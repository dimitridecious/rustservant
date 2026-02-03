#![allow(dead_code)] // Functions prepared for final !where command implementation

use crate::rustplus::app_map::Monument;

/// Convert world coordinates to grid notation (e.g., "G15")
pub fn coords_to_grid(x: f32, y: f32, map_size: u32) -> String {
    const CELL_SIZE: f32 = 150.0; // Standard Rust grid size in meters

    let col = (x / CELL_SIZE).floor() as u32;
    let row = (((map_size as f32 - y) / CELL_SIZE).floor() as i32 - 1).max(0) as u32;

    // Convert column to letter(s) - supports A-Z, then AA-AZ, BA-BZ, etc.
    let col_name = if col < 26 {
        format!("{}", (b'A' + col as u8) as char)
    } else {
        let first = col / 26 - 1;
        let second = col % 26;
        format!("{}{}", (b'A' + first as u8) as char, (b'A' + second as u8) as char)
    };

    format!("{}{}", col_name, row)
}

/// Calculate Euclidean distance between two points (for monument proximity)
pub fn calculate_distance(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    (dx * dx + dy * dy).sqrt()
}

/// Parse user event input to AppMarkerType enum value
pub fn parse_event_alias(input: &str) -> Option<i32> {
    match input {
        "heli" | "patrolheli" | "patrol" => Some(8), // PatrolHelicopter
        "cargo" | "cargoship" | "ship" => Some(5),   // CargoShip
        "crate" | "lockedcrate" => Some(6),          // Crate
        "chinook" | "ch47" => Some(4),               // CH47
        "largeoil" | "largerig" => Some(6),          // Crate (filtered by monument later)
        "smalloil" | "smallrig" => Some(6),          // Crate (filtered by monument later)
        _ => None,
    }
}

/// Find nearest monument within specified radius
pub fn get_monument_near(x: f32, y: f32, monuments: &[Monument], radius: f32) -> Option<String> {
    let mut nearest: Option<(String, f32)> = None;

    for monument in monuments {
        let distance = calculate_distance(x, y, monument.x, monument.y);
        if distance <= radius {
            if let Some((_, nearest_dist)) = nearest {
                if distance < nearest_dist {
                    nearest = Some((monument.token.clone(), distance));
                }
            } else {
                nearest = Some((monument.token.clone(), distance));
            }
        }
    }

    nearest.map(|(name, _)| name)
}

/// Determine map region from coordinates
pub fn get_map_region(x: f32, y: f32, map_size: u32) -> String {
    let map_f = map_size as f32;
    let third = map_f / 3.0;

    let horizontal = if x < third {
        "left"
    } else if x < third * 2.0 {
        "center"
    } else {
        "right"
    };

    let vertical = if y < third {
        "bottom"
    } else if y < third * 2.0 {
        "middle"
    } else {
        "top"
    };

    match (vertical, horizontal) {
        ("middle", "center") => "center".to_string(),
        ("top", "center") => "top".to_string(),
        ("bottom", "center") => "bottom".to_string(),
        ("middle", h) => h.to_string(),
        (v, h) => format!("{} {}", v, h),
    }
}
