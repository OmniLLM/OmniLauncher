use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use regex::Regex;
use std::sync::LazyLock;

static CONVERT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(\d+(?:\.\d+)?)\s*([a-z]+)\s+(?:to|in)\s+([a-z]+)$")
        .expect("static converter regex compiles")
});

pub struct UnitConverterPlugin;

#[async_trait]
impl Plugin for UnitConverterPlugin {
    fn name(&self) -> &str {
        "unit_converter"
    }

    fn description(&self) -> &str {
        "Convert units: distance, weight, temperature, data size"
    }

    fn keyword(&self) -> Option<&str> {
        Some("conv ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();

        // Try to parse "conv 5kg to lbs" or "5kg to lbs" style input
        let input = if let Some(stripped) = raw.strip_prefix("conv ") {
            stripped.trim()
        } else {
            // Also try direct pattern match e.g. "5kg to lbs"
            raw
        };

        if let Some(result) = self.parse_and_convert(input) {
            return vec![QueryResult {
                id: format!("conv:{}", input),
                title: result.clone(),
                subtitle: Some("Press Enter to copy result".to_string()),
                icon: Some("📐".to_string()),
                score: 100,
                action_type: "copy".to_string(),
                action_data: result,
                source: None,
            }];
        }

        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "convert_unit",
                "description": "Convert between units (distance, weight, temperature, data size)",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "value": {
                            "type": "number",
                            "description": "The numeric value to convert"
                        },
                        "from_unit": {
                            "type": "string",
                            "description": "Unit to convert from (e.g. km, mi, kg, lbs, c, f, gb)"
                        },
                        "to_unit": {
                            "type": "string",
                            "description": "Unit to convert to (e.g. km, mi, kg, lbs, c, f, gb)"
                        }
                    },
                    "required": ["value", "from_unit", "to_unit"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let value = args["value"].as_f64().unwrap_or(0.0);
        let from_unit = args["from_unit"].as_str().unwrap_or("").to_lowercase();
        let to_unit = args["to_unit"].as_str().unwrap_or("").to_lowercase();

        if let Some(result) = self.do_conversion(value, &from_unit, &to_unit) {
            format!("{} {} = {}", value, from_unit, result)
        } else {
            format!("Cannot convert from '{}' to '{}'", from_unit, to_unit)
        }
    }
}

impl UnitConverterPlugin {
    fn parse_and_convert(&self, input: &str) -> Option<String> {
        // Pattern: number + unit + optional "to" + unit
        if let Some(caps) = CONVERT_RE.captures(input) {
            let value: f64 = caps.get(1)?.as_str().parse().ok()?;
            let from_unit = caps.get(2)?.as_str().to_lowercase();
            let to_unit = caps.get(3)?.as_str().to_lowercase();
            return self.do_conversion(value, &from_unit, &to_unit);
        }
        None
    }

    fn do_conversion(&self, value: f64, from_unit: &str, to_unit: &str) -> Option<String> {
        let from_standard = self.standardize_unit(from_unit)?;
        let to_standard = self.standardize_unit(to_unit)?;

        // Distance conversions (base: meters)
        if let Some(from_m) = distance_to_meters(value, &from_standard) {
            if let Some(to_val) = meters_to_unit(from_m, &to_standard) {
                return Some(format!("{:.4} {}", to_val, to_standard));
            }
        }

        // Weight conversions (base: kilograms)
        if let Some(from_kg) = weight_to_kg(value, &from_standard) {
            if let Some(to_val) = kg_to_unit(from_kg, &to_standard) {
                return Some(format!("{:.4} {}", to_val, to_standard));
            }
        }

        // Temperature conversions
        if let Some(c) = temp_to_celsius(value, &from_standard) {
            let result = celsius_to_temp(c, &to_standard);
            return Some(format!("{:.2} {}", result, to_standard));
        }

        // Data size conversions (base: bytes)
        if let Some(from_b) = data_to_bytes(value, &from_standard) {
            if let Some(to_val) = bytes_to_unit(from_b, &to_standard) {
                return Some(format!("{:.4} {}", to_val, to_standard));
            }
        }

        None
    }

    fn standardize_unit(&self, unit: &str) -> Option<String> {
        let unit = unit.to_lowercase();
        match unit.as_str() {
            // Distance
            "km" | "kilometer" | "kilometers" | "k" => Some("km".to_string()),
            "mi" | "mile" | "miles" => Some("mi".to_string()),
            "m" | "meter" | "meters" => Some("m".to_string()),
            "ft" | "foot" | "feet" => Some("ft".to_string()),
            "cm" | "centimeter" | "centimeters" => Some("cm".to_string()),
            "in" | "inch" | "inches" => Some("in".to_string()),
            // Weight
            "kg" | "kilogram" | "kilograms" => Some("kg".to_string()),
            "lbs" | "lb" | "pound" | "pounds" => Some("lbs".to_string()),
            "g" | "gram" | "grams" => Some("g".to_string()),
            "oz" | "ounce" | "ounces" => Some("oz".to_string()),
            // Temperature
            "c" | "celsius" => Some("c".to_string()),
            "f" | "fahrenheit" => Some("f".to_string()),
            "kelvin" => Some("k".to_string()),
            // Data size
            "gb" | "gigabyte" | "gigabytes" => Some("gb".to_string()),
            "mb" | "megabyte" | "megabytes" => Some("mb".to_string()),
            "kb" | "kilobyte" | "kilobytes" => Some("kb".to_string()),
            "tb" | "terabyte" | "terabytes" => Some("tb".to_string()),
            _ => None,
        }
    }
}

// --- Distance conversion helpers (base: meters) ---

fn distance_to_meters(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "km" => Some(value * 1000.0),
        "mi" => Some(value * 1609.344),
        "m" => Some(value),
        "ft" => Some(value * 0.3048),
        "cm" => Some(value * 0.01),
        "in" => Some(value * 0.0254),
        _ => None,
    }
}

fn meters_to_unit(meters: f64, unit: &str) -> Option<f64> {
    match unit {
        "km" => Some(meters / 1000.0),
        "mi" => Some(meters / 1609.344),
        "m" => Some(meters),
        "ft" => Some(meters / 0.3048),
        "cm" => Some(meters / 0.01),
        "in" => Some(meters / 0.0254),
        _ => None,
    }
}

// --- Weight conversion helpers (base: kilograms) ---

fn weight_to_kg(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "kg" => Some(value),
        "lbs" => Some(value * 0.45359237),
        "g" => Some(value * 0.001),
        "oz" => Some(value * 0.028349523125),
        _ => None,
    }
}

fn kg_to_unit(kg: f64, unit: &str) -> Option<f64> {
    match unit {
        "kg" => Some(kg),
        "lbs" => Some(kg / 0.45359237),
        "g" => Some(kg / 0.001),
        "oz" => Some(kg / 0.028349523125),
        _ => None,
    }
}

// --- Temperature conversion helpers (base: celsius) ---

fn temp_to_celsius(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "c" => Some(value),
        "f" => Some((value - 32.0) * 5.0 / 9.0),
        "k" => Some(value - 273.15),
        _ => None,
    }
}

fn celsius_to_temp(c: f64, unit: &str) -> f64 {
    match unit {
        "c" => c,
        "f" => c * 9.0 / 5.0 + 32.0,
        "k" => c + 273.15,
        _ => f64::NAN,
    }
}

// --- Data size conversion helpers (base: bytes) ---

fn data_to_bytes(value: f64, unit: &str) -> Option<f64> {
    match unit {
        "tb" => Some(value * 1024.0 * 1024.0 * 1024.0 * 1024.0),
        "gb" => Some(value * 1024.0 * 1024.0 * 1024.0),
        "mb" => Some(value * 1024.0 * 1024.0),
        "kb" => Some(value * 1024.0),
        _ => None,
    }
}

fn bytes_to_unit(bytes: f64, unit: &str) -> Option<f64> {
    match unit {
        "tb" => Some(bytes / (1024.0 * 1024.0 * 1024.0 * 1024.0)),
        "gb" => Some(bytes / (1024.0 * 1024.0 * 1024.0)),
        "mb" => Some(bytes / (1024.0 * 1024.0)),
        "kb" => Some(bytes / 1024.0),
        _ => None,
    }
}
