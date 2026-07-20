use anyhow::Result;
use wasmtime::component::{Component, HasSelf};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../wit",
    world: "repro-host-world",
});

struct Host {
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for Host {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

fn pad(s: String, width: usize) -> String {
    if s.len() >= width {
        return s;
    }
    format!("{}{}", s, "0".repeat(width - s.len()))
}

const PAD: usize = 500;
const TAGS_PER: u32 = 5;

impl repro::gc_bug::source::Host for Host {
    fn get_items(
        &mut self,
        n: u32,
        tags_per: u32,
    ) -> Vec<repro::gc_bug::source::Item> {
        (0..n)
            .map(|i| repro::gc_bug::source::Item {
                id:   pad(format!("item-{:04}", i), PAD),
                tags: (0..tags_per)
                    .map(|j| pad(format!("tag-{:04}-{}", i, j), PAD))
                    .collect(),
            })
            .collect()
    }
}

fn report(label: &str, n: u32, results: &[bool]) -> bool {
    let corrupted: Vec<usize> = results
        .iter()
        .enumerate()
        .filter(|(_, &ok)| !ok)
        .map(|(i, _)| i)
        .collect();
    if corrupted.is_empty() {
        println!("{label}: PASS — all {}/{n} items intact", results.len());
        true
    } else {
        eprintln!(
            "{label}: FAIL — {}/{n} items corrupted at indices {:?}{}",
            corrupted.len(),
            &corrupted[..corrupted.len().min(10)],
            if corrupted.len() > 10 { " ..." } else { "" }
        );
        false
    }
}

fn main() -> Result<()> {
    let wasm_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../repro.wasm".to_string());
    let n: u32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(80);

    let engine = Engine::new(Config::new().wasm_component_model(true))?;
    let component = Component::from_file(&engine, &wasm_path)?;

    let mut linker: wasmtime::component::Linker<Host> =
        wasmtime::component::Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    ReproHostWorld::add_to_linker::<_, HasSelf<_>>(&mut linker, |h| h)?;

    let make_store = || Store::new(&engine, Host {
        wasi: WasiCtxBuilder::new().inherit_stdio().build(),
        table: ResourceTable::new(),
    });

    // Each export gets its own fresh WASM instance so the GC heap state of one
    // cannot affect the other.
    let mut store1 = make_store();
    let inst1 = ReproHostWorld::instantiate(&mut store1, &component, &linker)?;
    let r1 = inst1.repro_gc_bug_checker().call_check(&mut store1, n, TAGS_PER)?;
    let ok1 = report("check      ", n, &r1);

    let mut store2 = make_store();
    let inst2 = ReproHostWorld::instantiate(&mut store2, &component, &linker)?;
    let r2 = inst2.repro_gc_bug_checker().call_check_fixed(&mut store2, n, TAGS_PER)?;
    let ok2 = report("check-fixed", n, &r2);

    if !ok1 && ok2 {
        eprintln!("\ncheck FAILs, check-fixed PASSes — bug + fix confirmed.");
        std::process::exit(1);
    }
    if ok1 {
        eprintln!("\ncheck PASSed — GC did not fire; try a larger n.");
    }
    if !ok2 {
        eprintln!("\ncheck-fixed FAILed — unexpected; sentinel may be too small.");
        std::process::exit(2);
    }
    Ok(())
}
