//! Location detection and OpenStreetMap URLs for the event details view.
//!
//! Coordinates are parsed locally; address-like strings are geocoded with
//! Photon (an OSM-based geocoder with CORS support). The map itself is the
//! standard OpenStreetMap embed.

/// Parse `52.5163, 13.3777`, `geo:52.5163,13.3777`, `@52.5163,13.3777,15z`
/// or `52.5163 13.3777` (also with N/S/E/W suffixes).
pub fn parse_coords(input: &str) -> Option<(f64, f64)> {
    let mut s = input.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("geo:") {
        s = &s[s.len() - rest.len()..];
    } else if let Some(rest) = s.strip_prefix('@') {
        s = rest;
    }
    // Google-Maps-style "@lat,lon,15z": keep the first two components.
    let s = s.split(',').take(2).collect::<Vec<_>>().join(",");
    let parts: Vec<&str> = s
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 2 {
        return None;
    }
    let lat = parse_lat(parts[0])?;
    let lon = parse_lon(parts[1])?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some((lat, lon))
}

fn parse_lat(part: &str) -> Option<f64> {
    let (num, sign) = match part.chars().last()? {
        'N' | 'n' => (&part[..part.len() - 1], 1.0),
        'S' | 's' => (&part[..part.len() - 1], -1.0),
        _ => (part, 1.0),
    };
    num.parse::<f64>().ok().map(|v| v * sign)
}

fn parse_lon(part: &str) -> Option<f64> {
    let (num, sign) = match part.chars().last()? {
        'E' | 'e' => (&part[..part.len() - 1], 1.0),
        'W' | 'w' => (&part[..part.len() - 1], -1.0),
        _ => (part, 1.0),
    };
    num.parse::<f64>().ok().map(|v| v * sign)
}

/// A conservative heuristic: only strings that look like a street address
/// (comma, street suffix, or three+ words ending in a house number) are sent
/// to the geocoder. Plain venue names ("Library", "Café Nord") stay local.
pub fn looks_like_address(input: &str) -> bool {
    let s = input.trim();
    if s.is_empty() || parse_coords(s).is_some() {
        return false;
    }
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if s.contains(',') {
        return true;
    }
    let lower = s.to_lowercase();
    const SUFFIXES: &[&str] = &[
        "street", " st.", " st ", "road", " rd.", " rd ", "avenue", " ave.", " ave ",
        "lane", " ln.", "drive", " dr.", "platz", "straße", "strasse", "str.", "weg",
        "gasse", "allee", "boulevard", "blvd", "ring", "ufer", "damm", "steig", "pfad",
        "chaussee", "rue ", "via ", "calle ", "piazza ", "quai ",
    ];
    if SUFFIXES.iter().any(|suffix| lower.contains(suffix)) {
        return true;
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    words.len() >= 3
        && words
            .last()
            .map(|w| w.chars().any(|c| c.is_ascii_digit()))
            .unwrap_or(false)
}

/// OSM embed with a marker and a ~400 m bounding box.
pub fn osm_embed(lat: f64, lon: f64) -> String {
    let d = 0.004;
    format!(
        "https://www.openstreetmap.org/export/embed.html?bbox={:.6}%2C{:.6}%2C{:.6}%2C{:.6}&layer=mapnik&marker={:.6}%2C{:.6}",
        lon - d,
        lat - d,
        lon + d,
        lat + d,
        lat,
        lon
    )
}

/// OSM permalink centered on the marker.
pub fn osm_link(lat: f64, lon: f64) -> String {
    format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=16/{lat:.6}/{lon:.6}")
}

/// OSM search page for a plain-text location.
pub fn osm_search(query: &str) -> String {
    let q = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("query", query.trim())
        .finish();
    format!("https://www.openstreetmap.org/search?{q}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_coordinate_formats() {
        assert_eq!(parse_coords("52.5163, 13.3777"), Some((52.5163, 13.3777)));
        assert_eq!(parse_coords("geo:52.5163,13.3777"), Some((52.5163, 13.3777)));
        assert_eq!(parse_coords("@52.5163,13.3777,15z"), Some((52.5163, 13.3777)));
        assert_eq!(parse_coords("52.5163 13.3777"), Some((52.5163, 13.3777)));
        assert_eq!(parse_coords("52.5163N 13.3777E"), Some((52.5163, 13.3777)));
        assert_eq!(parse_coords("33.8688S 151.2093E"), Some((-33.8688, 151.2093)));
    }

    #[test]
    fn rejects_non_coordinates() {
        assert_eq!(parse_coords("Café Nord"), None);
        assert_eq!(parse_coords("Room 4"), None);
        assert_eq!(parse_coords("52.5163"), None);
        assert_eq!(parse_coords("91, 13"), None);
        assert_eq!(parse_coords("52, 181"), None);
    }

    #[test]
    fn detects_address_like_locations() {
        assert!(looks_like_address("Unter den Linden 77"));
        assert!(looks_like_address("Hauptstraße 5"));
        assert!(looks_like_address("Alexanderplatz 1, Berlin"));
        assert!(!looks_like_address("Room 4"));
        assert!(!looks_like_address("Café Nord"));
        assert!(!looks_like_address("52.5163, 13.3777"));
    }

    #[test]
    fn builds_osm_urls() {
        let embed = osm_embed(52.5163, 13.3777);
        assert!(embed.starts_with("https://www.openstreetmap.org/export/embed.html?"));
        assert!(embed.contains("marker=52.516300%2C13.377700"));
        let link = osm_link(52.5163, 13.3777);
        assert!(link.contains("mlat=52.516300"));
        assert!(link.contains("#map=16/"));
        assert!(osm_search("Café Nord").contains("query=Caf%C3%A9+Nord"));
    }
}
