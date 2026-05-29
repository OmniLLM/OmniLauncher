use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct ColorPickerPlugin;

#[async_trait]
impl Plugin for ColorPickerPlugin {
    fn name(&self) -> &str {
        "color_picker"
    }

    fn description(&self) -> &str {
        "Convert colors between hex, rgb, hsl formats"
    }

    fn keyword(&self) -> Option<&str> {
        Some("color ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("color ")
            .unwrap_or("")
            .trim()
            .to_string();
        if term.is_empty() {
            return vec![QueryResult {
                id: "color:help".to_string(),
                title: "Color Converter".to_string(),
                subtitle: Some("Enter hex (#ff0000), rgb(255,0,0), or color name".to_string()),
                icon: Some("🎨".to_string()),
                score: 50,
                action_type: "copy".to_string(),
                action_data: String::new(),
            }];
        }

        let mut results = vec![];

        // Try parse hex
        if let Some((r, g, b)) = parse_hex(&term) {
            let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
            let rgb = format!("rgb({}, {}, {})", r, g, b);
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let hsl = format!("hsl({}, {}%, {}%)", h, s, l);
            results.push(make_result("HEX", &hex));
            results.push(make_result("RGB", &rgb));
            results.push(make_result("HSL", &hsl));
        }
        // Try parse rgb(r,g,b)
        else if let Some((r, g, b)) = parse_rgb(&term) {
            let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
            let rgb = format!("rgb({}, {}, {})", r, g, b);
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let hsl = format!("hsl({}, {}%, {}%)", h, s, l);
            results.push(make_result("HEX", &hex));
            results.push(make_result("RGB", &rgb));
            results.push(make_result("HSL", &hsl));
        }
        // Try named color
        else if let Some((r, g, b)) = named_color(&term) {
            let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
            let rgb = format!("rgb({}, {}, {})", r, g, b);
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let hsl = format!("hsl({}, {}%, {}%)", h, s, l);
            results.push(make_result("HEX", &hex));
            results.push(make_result("RGB", &rgb));
            results.push(make_result("HSL", &hsl));
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "color_picker",
                "description": "Convert colors between hex, rgb, hsl formats",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "color": { "type": "string", "description": "Color to convert: hex (#ff0000), rgb(255,0,0), or color name (red, blue, ...)" }
                    },
                    "required": ["color"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let color = args["color"].as_str().unwrap_or("").trim().to_string();
        if color.is_empty() {
            return "Error: 'color' parameter is required".to_string();
        }
        if let Some((r, g, b)) = parse_hex(&color)
            .or_else(|| parse_rgb(&color))
            .or_else(|| named_color(&color))
        {
            let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
            let rgb = format!("rgb({}, {}, {})", r, g, b);
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let hsl = format!("hsl({}, {}%, {}%)", h, s, l);
            format!("HEX: {}\nRGB: {}\nHSL: {}", hex, rgb, hsl)
        } else {
            format!(
                "Could not parse color: '{}'. Use hex (#ff0000), rgb(255,0,0), or a color name.",
                color
            )
        }
    }
}

fn make_result(format: &str, value: &str) -> QueryResult {
    QueryResult {
        id: format!("color:{}:{}", format, value),
        title: value.to_string(),
        subtitle: Some(format!("{} – click to copy", format)),
        icon: Some("🎨".to_string()),
        score: 80,
        action_type: "copy".to_string(),
        action_data: value.to_string(),
    }
}

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some((r, g, b))
    } else if s.len() == 3 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    let inner = s
        .strip_prefix("rgb(")
        .or_else(|| s.strip_prefix("RGB("))
        .and_then(|s| s.strip_suffix(')'))?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() == 3 {
        let r = parts[0].trim().parse::<u8>().ok()?;
        let g = parts[1].trim().parse::<u8>().ok()?;
        let b = parts[2].trim().parse::<u8>().ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u16, u16, u16) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0, 0, (l * 100.0).round() as u16);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f64::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if (max - g).abs() < f64::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (
        (h * 360.0).round() as u16,
        (s * 100.0).round() as u16,
        (l * 100.0).round() as u16,
    )
}

fn named_color(name: &str) -> Option<(u8, u8, u8)> {
    match name.to_lowercase().as_str() {
        "red" => Some((255, 0, 0)),
        "green" => Some((0, 128, 0)),
        "blue" => Some((0, 0, 255)),
        "white" => Some((255, 255, 255)),
        "black" => Some((0, 0, 0)),
        "yellow" => Some((255, 255, 0)),
        "cyan" => Some((0, 255, 255)),
        "magenta" => Some((255, 0, 255)),
        "orange" => Some((255, 165, 0)),
        "purple" => Some((128, 0, 128)),
        "pink" => Some((255, 192, 203)),
        "gray" | "grey" => Some((128, 128, 128)),
        "brown" => Some((165, 42, 42)),
        "navy" => Some((0, 0, 128)),
        "teal" => Some((0, 128, 128)),
        "lime" => Some((0, 255, 0)),
        "coral" => Some((255, 127, 80)),
        "salmon" => Some((250, 128, 114)),
        "gold" => Some((255, 215, 0)),
        "silver" => Some((192, 192, 192)),
        _ => None,
    }
}
