
use bindings::exports::docs::getcoordsfromaddressworld::getcoordsfromaddress::{
    Address, Coordinates,
};
use bindings::wasi::cli::environment;
use bindings::wasi::http::outgoing_handler;
use bindings::wasi::http::types::{Fields, IncomingBody, Method, OutgoingRequest, Scheme};
use bindings::wasi::io::poll;

mod bindings {
    wit_bindgen::generate!({
        world: "getcoordsfromaddressworld",
        path: "wit",
        with: {
            "wasi:cli/environment@0.2.0": generate,
            "wasi:http/outgoing-handler@0.2.0": generate,
            "wasi:http/types@0.2.0": generate,
            "wasi:io/poll@0.2.0": generate,
            "wasi:io/streams@0.2.0": generate,
            "wasi:io/error@0.2.0": generate,
            "wasi:clocks/monotonic-clock@0.2.0": generate,
        },
    });

    use super::GetCoordsFromAddressComponent;
    export!(GetCoordsFromAddressComponent);
}

struct GetCoordsFromAddressComponent;

/// URL-encode a string for use in query parameters.
fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{:02X}", b));
            }
        }
    }
    result
}

/// Read the full response body from an incoming HTTP response.
fn read_body(body: IncomingBody) -> Vec<u8> {
    let stream = body.stream().expect("failed to get input stream");
    let mut buf = Vec::new();
    loop {
        let pollable = stream.subscribe();
        poll::poll(&[&pollable]);
        match stream.read(64 * 1024) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                buf.extend_from_slice(&chunk);
            }
            Err(_) => break,
        }
    }
    drop(stream);
    IncomingBody::finish(body);
    buf
}

/// Extract a f64 value from a JSON string by key (e.g. "lat" or "lng").
fn extract_json_f64(json: &str, key: &str) -> Option<f64> {
    let search = format!("\"{}\"", key);
    let idx = json.find(&search)?;
    let after_key = &json[idx + search.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let trimmed = after_colon.trim_start();
    let end = trimmed.find(|c: char| c != '-' && c != '.' && !c.is_ascii_digit())?;
    trimmed[..end].parse::<f64>().ok()
}

impl bindings::exports::docs::getcoordsfromaddressworld::getcoordsfromaddress::Guest
    for GetCoordsFromAddressComponent
{
    fn getcoordsfromaddress(address: Address) -> Coordinates {
        // Read the Google Maps API key from environment
        let env_vars = environment::get_environment();
        let api_key = env_vars
            .iter()
            .find(|(k, _)| k == "GOOGLE_MAPS_API_KEY")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        // Build the address string from the record fields
        let mut parts = vec![
            format!("{} {}", address.streetnumber, address.street),
            address.town.clone(),
            address.zip.clone(),
        ];
        if let Some(ref region) = address.region {
            parts.push(region.clone());
        }
        let address_str = parts.join(", ");

        // Construct the Google Geocoding API request path
        let path = format!(
            "/maps/api/geocode/json?address={}&key={}",
            url_encode(&address_str),
            url_encode(&api_key)
        );

        // Build and send the outgoing HTTP request
        let headers = Fields::new();
        let request = OutgoingRequest::new(headers);
        request.set_method(&Method::Get).expect("set method");
        request.set_scheme(Some(&Scheme::Https)).expect("set scheme");
        request
            .set_authority(Some("maps.googleapis.com"))
            .expect("set authority");
        request
            .set_path_with_query(Some(&path))
            .expect("set path");

        let future_response =
            outgoing_handler::handle(request, None).expect("failed to send request");

        // Block until the response arrives
        let pollable = future_response.subscribe();
        poll::poll(&[&pollable]);

        let response = future_response
            .get()
            .expect("response not ready")
            .expect("response outer error")
            .expect("HTTP error");

        let status = response.status();
        let body = response.consume().expect("failed to consume body");
        let body_bytes = read_body(body);
        let body_str = String::from_utf8_lossy(&body_bytes);

        if status != 200 {
            return Coordinates {
                latitude: 0.0,
                longitude: 0.0,
            };
        }

        // Parse lat/lng from the Google Geocoding JSON response
        // Response contains: "location": { "lat": ..., "lng": ... }
        let lat = extract_json_f64(&body_str, "lat").unwrap_or(0.0);
        let lng = extract_json_f64(&body_str, "lng").unwrap_or(0.0);

        Coordinates {
            latitude: lat,
            longitude: lng,
        }
    }
}   
