package main

import (
	"encoding/json"
	"strconv"
	"strings"

	"go.bytecodealliance.org/cm"

	incominghandler "hotel-components/components/frontend/wasi/http/incoming-handler"
	"hotel-components/components/frontend/wasi/http/types"

	attractions "hotel-components/components/frontend/hotel/api/attractions"
	profile "hotel-components/components/frontend/hotel/api/profile"
	recommendation "hotel-components/components/frontend/hotel/api/recommendation"
	reservation "hotel-components/components/frontend/hotel/api/reservation"
	review "hotel-components/components/frontend/hotel/api/review"
	search "hotel-components/components/frontend/hotel/api/search"
	user "hotel-components/components/frontend/hotel/api/user"
)

func main() {}

func init() {
	incominghandler.Exports.Handle = handle
}

func handle(req incominghandler.IncomingRequest, responseOut incominghandler.ResponseOutparam) {
	path, query := parsePQ(req.PathWithQuery())
	req.ResourceDrop()
	body, ct := dispatch(path, query)
	sendResponse(responseOut, 200, ct, body)
}

func parsePQ(pq cm.Option[string]) (path, query string) {
	s := pq.Some()
	if s == nil {
		return "/", ""
	}
	full := *s
	if i := strings.IndexByte(full, '?'); i >= 0 {
		return full[:i], full[i+1:]
	}
	return full, ""
}

func parseQuery(raw string) map[string]string {
	m := make(map[string]string)
	for _, part := range strings.Split(raw, "&") {
		if i := strings.IndexByte(part, '='); i >= 0 {
			m[part[:i]] = part[i+1:]
		}
	}
	return m
}

func dispatch(path, query string) (body, contentType string) {
	q := parseQuery(query)
	switch path {
	case "/", "":
		return indexHTML, "text/html; charset=utf-8"
	case "/hotels":
		return handleHotels(q), "application/json"
	case "/recommendations":
		return handleRecommendations(q), "application/json"
	case "/user":
		return handleUser(q), "application/json"
	case "/review":
		return handleReview(q), "application/json"
	case "/restaurants":
		return handleRestaurants(q), "application/json"
	case "/museums":
		return handleMuseums(q), "application/json"
	case "/cinema":
		return handleCinema(q), "application/json"
	case "/reservation":
		return handleReservation(q), "application/json"
	default:
		return `{"message":"not found"}`, "application/json"
	}
}

// ---- JSON helpers ----

type msgResponse struct {
	Message string `json:"message"`
}

func msgJSON(msg string) string {
	b, _ := json.Marshal(msgResponse{Message: msg})
	return string(b)
}

type geoGeometry struct {
	Type        string     `json:"type"`
	Coordinates [2]float64 `json:"coordinates"` // [lon, lat]
}

type geoProperties struct {
	Name        string `json:"name"`
	PhoneNumber string `json:"phone_number"`
}

type geoFeature struct {
	Type       string        `json:"type"`
	ID         string        `json:"id"`
	Properties geoProperties `json:"properties"`
	Geometry   geoGeometry   `json:"geometry"`
}

type geoCollection struct {
	Type     string       `json:"type"`
	Features []geoFeature `json:"features"`
}

func geoJSONResponse(hotels []profile.Hotel) string {
	features := make([]geoFeature, len(hotels))
	for i, h := range hotels {
		features[i] = geoFeature{
			Type: "Feature",
			ID:   h.ID,
			Properties: geoProperties{
				Name:        h.Name,
				PhoneNumber: h.PhoneNumber,
			},
			Geometry: geoGeometry{
				Type:        "Point",
				Coordinates: [2]float64{h.Addr.Lon, h.Addr.Lat},
			},
		}
	}
	b, _ := json.Marshal(geoCollection{Type: "FeatureCollection", Features: features})
	return string(b)
}

// ---- Route handlers ----

func handleHotels(q map[string]string) string {
	lat, _ := strconv.ParseFloat(q["lat"], 64)
	lon, _ := strconv.ParseFloat(q["lon"], 64)
	inDate := q["inDate"]
	outDate := q["outDate"]

	nearbyRaw := search.Nearby(lat, lon, inDate, outDate).Slice()
	nearbyIDs := make([]string, len(nearbyRaw))
	for i, id := range nearbyRaw {
		nearbyIDs[i] = id
	}

	availRaw := reservation.CheckAvailability(cm.ToList(nearbyIDs), inDate, outDate, 1).Slice()
	availIDs := make([]string, len(availRaw))
	for i, id := range availRaw {
		availIDs[i] = id
	}

	hotels := profile.GetProfiles(cm.ToList(availIDs)).Slice()
	return geoJSONResponse(hotels)
}

func handleRecommendations(q map[string]string) string {
	lat, _ := strconv.ParseFloat(q["lat"], 64)
	lon, _ := strconv.ParseFloat(q["lon"], 64)

	var req recommendation.Requirement
	switch q["require"] {
	case "rate":
		req = recommendation.RequirementRate
	case "price":
		req = recommendation.RequirementPrice
	default:
		req = recommendation.RequirementDistance
	}

	recRaw := recommendation.Recommend(req, lat, lon).Slice()
	recIDs := make([]string, len(recRaw))
	for i, id := range recRaw {
		recIDs[i] = id
	}

	hotels := profile.GetProfiles(cm.ToList(recIDs)).Slice()
	return geoJSONResponse(hotels)
}

func handleUser(q map[string]string) string {
	if user.CheckUser(q["username"], q["password"]) {
		return msgJSON("Login successfully!")
	}
	return msgJSON("Failed. Please check your username and password. ")
}

func handleReview(q map[string]string) string {
	user.CheckUser(q["username"], q["password"])
	hotelID := q["hotelId"]
	reviews := review.GetReviews(hotelID).Slice()
	if len(reviews) == 0 {
		return msgJSON("Failed. No Reviews. ")
	}
	return msgJSON("Have reviews = " + strconv.Itoa(len(reviews)))
}

func handleRestaurants(q map[string]string) string {
	user.CheckUser(q["username"], q["password"])
	hotelID := q["hotelId"]
	ids := attractions.NearbyRest(hotelID).Slice()
	if len(ids) == 0 {
		return msgJSON("Failed. No Restaurants. ")
	}
	return msgJSON("Have restaurants = " + strconv.Itoa(len(ids)))
}

func handleMuseums(q map[string]string) string {
	user.CheckUser(q["username"], q["password"])
	hotelID := q["hotelId"]
	ids := attractions.NearbyMus(hotelID).Slice()
	if len(ids) == 0 {
		return msgJSON("Failed. No Museums. ")
	}
	return msgJSON("Have museums = " + strconv.Itoa(len(ids)))
}

func handleCinema(q map[string]string) string {
	user.CheckUser(q["username"], q["password"])
	hotelID := q["hotelId"]
	ids := attractions.NearbyCinema(hotelID).Slice()
	if len(ids) == 0 {
		return msgJSON("Failed. No Cinemas. ")
	}
	return msgJSON("Have cinemas = " + strconv.Itoa(len(ids)))
}

func handleReservation(q map[string]string) string {
	inDate := q["inDate"]
	outDate := q["outDate"]
	hotelID := q["hotelId"]
	customerName := q["customerName"]
	number, _ := strconv.ParseInt(q["number"], 10, 32)

	msg := "Reserve successfully!"
	if !user.CheckUser(q["username"], q["password"]) {
		msg = "Failed. Please check your username and password. "
	}

	res := reservation.MakeReservation(hotelID, customerName, inDate, outDate, int32(number)).Slice()
	if len(res) == 0 {
		msg = "Failed. Already reserved. "
	}
	return msgJSON(msg)
}

// ---- HTTP response ----

func sendResponse(responseOut types.ResponseOutparam, status uint16, contentType, body string) {
	headers := types.NewFields()
	headers.Append("content-type", types.FieldValue(cm.ToList([]uint8(contentType))))

	resp := types.NewOutgoingResponse(headers)
	resp.SetStatusCode(types.StatusCode(status))

	bodyRes := resp.Body()
	outBody := *bodyRes.OK()

	writeRes := outBody.Write()
	stream := *writeRes.OK()

	stream.BlockingWriteAndFlush(cm.ToList([]uint8(body)))
	stream.ResourceDrop()

	types.OutgoingBodyFinish(outBody, cm.None[types.Trailers]())

	var okResult cm.Result[types.ErrorCodeShape, types.OutgoingResponse, types.ErrorCode]
	*okResult.OK() = resp
	types.ResponseOutparamSet(responseOut, okResult)
}

// ---- Static HTML ----

const indexHTML = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Hotel Reservation</title>
<style>
  body { font-family: sans-serif; max-width: 860px; margin: 2em auto; padding: 0 1em; color: #222; }
  h1 { margin-bottom: .2em; }
  h2 { border-bottom: 1px solid #ddd; padding-bottom: .25em; margin-top: 1.6em; }
  form { display: grid; grid-template-columns: max-content 1fr; gap: .35em .8em; align-items: center; margin-bottom: .5em; }
  label { text-align: right; }
  input { padding: .3em .5em; width: 220px; border: 1px solid #bbb; border-radius: 3px; }
  button { grid-column: 2; margin-top: .3em; padding: .4em 1.2em; cursor: pointer; }
  #result { background: #f6f6f6; border: 1px solid #ddd; border-radius: 4px; padding: 1em; white-space: pre-wrap; word-break: break-all; min-height: 3em; margin-top: 1em; }
</style>
</head>
<body>
<h1>Hotel Reservation</h1>

<h2>Search Hotels</h2>
<form onsubmit="callApi(event,'/hotels')">
  <label>Lat</label>     <input name="lat"     value="37.7749">
  <label>Lon</label>     <input name="lon"     value="-122.4194">
  <label>In Date</label> <input name="inDate"  value="2015-04-09">
  <label>Out Date</label><input name="outDate" value="2015-04-10">
  <button>Search</button>
</form>

<h2>Recommendations</h2>
<form onsubmit="callApi(event,'/recommendations')">
  <label>Lat</label>    <input name="lat"     value="37.7749">
  <label>Lon</label>    <input name="lon"     value="-122.4194">
  <label>Require</label><input name="require" value="dis" placeholder="dis | rate | price">
  <button>Get Recommendations</button>
</form>

<h2>Login</h2>
<form onsubmit="callApi(event,'/user')">
  <label>Username</label><input name="username" value="Cornell_0">
  <label>Password</label><input name="password" value="1111111111">
  <button>Login</button>
</form>

<h2>Make Reservation</h2>
<form onsubmit="callApi(event,'/reservation')">
  <label>Username</label>     <input name="username"     value="Cornell_0">
  <label>Password</label>     <input name="password"     value="1111111111">
  <label>Hotel ID</label>     <input name="hotelId"      value="1">
  <label>Customer Name</label><input name="customerName" value="Cornell_0">
  <label>In Date</label>      <input name="inDate"       value="2015-04-09">
  <label>Out Date</label>     <input name="outDate"      value="2015-04-10">
  <label>Room Number</label>  <input name="number"       value="1" type="number">
  <button>Reserve</button>
</form>

<h2>Reviews</h2>
<form onsubmit="callApi(event,'/review')">
  <label>Username</label><input name="username" value="Cornell_0">
  <label>Password</label><input name="password" value="1111111111">
  <label>Hotel ID</label><input name="hotelId"  value="1">
  <button>Get Reviews</button>
</form>

<h2>Nearby Attractions</h2>
<form onsubmit="callApi(event,'/restaurants')">
  <label>Username</label><input name="username" value="Cornell_0">
  <label>Password</label><input name="password" value="1111111111">
  <label>Hotel ID</label><input name="hotelId"  value="1">
  <button>Restaurants</button>
</form>
<form onsubmit="callApi(event,'/museums')">
  <label>Username</label><input name="username" value="Cornell_0">
  <label>Password</label><input name="password" value="1111111111">
  <label>Hotel ID</label><input name="hotelId"  value="1">
  <button>Museums</button>
</form>
<form onsubmit="callApi(event,'/cinema')">
  <label>Username</label><input name="username" value="Cornell_0">
  <label>Password</label><input name="password" value="1111111111">
  <label>Hotel ID</label><input name="hotelId"  value="1">
  <button>Cinema</button>
</form>

<h2>Result</h2>
<pre id="result">(submit a form above)</pre>

<script>
function callApi(e, path) {
  e.preventDefault();
  var params = new URLSearchParams(new FormData(e.target)).toString();
  fetch(path + '?' + params)
    .then(function(r){ return r.json(); })
    .then(function(j){ document.getElementById('result').textContent = JSON.stringify(j, null, 2); })
    .catch(function(err){ document.getElementById('result').textContent = String(err); });
}
</script>
</body>
</html>`
