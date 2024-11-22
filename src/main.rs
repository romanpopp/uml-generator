use regex::Regex;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::{env, fs};
use walkdir::WalkDir;

static PROPERTY_REGEX: &str = r"(?P<visibility>public|private|protected)\s*(?P<modifier>static|virtual|abstract|override|async|sealed|extern|unsafe|partial|readonly|event)?\s*(?P<type>[^\s]+)?\s+(?P<name>[^\s]+)";
static METHOD_REGEX: &str = r"(?P<visibility>public|private|protected)\s*(?P<modifiers>static|virtual|abstract|override|async|sealed|extern|unsafe|partial|readonly)?\s*(?P<return_type>[^\s]+)?\s+(?P<name>[^\s]+)\((?P<params>[^\)]*)\)";
static ENUM_REGEX: &str = r"enum\s+(?P<name>\w+)\s*\{(?P<values>[^}]*)\}?";

#[derive(Debug, PartialEq)]
enum ArrowType {
    Inheritance, // --|>
    Realization, // ..|>
    Aggregation, // --o
    Composition, // --*
}

#[derive(Debug)]
struct Arrow {
    start: String,
    end: String,
    arrow_type: ArrowType,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let current_dir = &args[1];
        let umlpath = Path::new(current_dir).join("uml.mmd");
        let mut file_contents = String::new();
        file_contents.push_str("classDiagram\ndirection RL");
        let mut class_list: Vec<String> = Vec::new();
        let mut arrow_list: Vec<Arrow> = Vec::new();

        for namespace in fs::read_dir(current_dir).unwrap() {
            let namespace = namespace.unwrap().path();
            let dir_name = namespace
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("file read error");
            if namespace.is_dir()
                && !dir_name.contains(".")
                && !dir_name.to_lowercase().contains("test")
            {
                file_contents.push_str(&format!("\nnamespace {} {{", dir_name));
                for entry in WalkDir::new(namespace) {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("cs")
                        || path.extension().and_then(|s| s.to_str()) == Some("xaml")
                    {
                        println!("parsing file: {}", path.display());
                        let contents = fs::read_to_string(path).expect("file read error");
                        file_contents.push_str(
                            parse_file(
                                contents,
                                path.file_stem()
                                    .and_then(|stem| stem.to_str())
                                    .expect("file read error"),
                                &mut class_list,
                                &mut arrow_list,
                            )
                            .as_str(),
                        );
                    }
                }
                file_contents.push_str("\n}");
            }
        }
        file_contents = file_contents.replace("~~get~~, ~~set~~", "~~get + set~~");
        // putting arrows at end, if they were valid
        for arrow in arrow_list {
            if (class_list.contains(&arrow.start) && class_list.contains(&arrow.end)
                || arrow.arrow_type == ArrowType::Inheritance
                || arrow.arrow_type == ArrowType::Realization)
                && !(arrow.start == arrow.end)
            {
                file_contents.push_str(&format!(
                    "\n{} {} {}",
                    arrow.start,
                    match arrow.arrow_type {
                        ArrowType::Inheritance => "--|>",
                        ArrowType::Realization => "..|>",
                        ArrowType::Aggregation => "\"1\" --* \"1\"",
                        ArrowType::Composition => "\"0..*\" --o \"1\"",
                    },
                    arrow.end,
                ));
            }
        }
        let mut file = File::create(&umlpath).expect("error creating file");
        file.write_all(file_contents.as_bytes())
            .expect("error writing contents to file");
    } else {
        println!("no directory to search");
    }
    println!("uml.mmd created");
}

fn parse_file(
    contents: String,
    filename: &str,
    class_list: &mut Vec<String>,
    arrow_list: &mut Vec<Arrow>,
) -> String {
    let property_regex = Regex::new(PROPERTY_REGEX).unwrap();
    let method_regex = Regex::new(METHOD_REGEX).unwrap();
    let enum_regex = Regex::new(ENUM_REGEX).unwrap();
    let mut needs_close_brace = false;
    let mut class_name = String::new();
    let mut data_context: Option<String> = None;

    let mut file_contents = String::new();
    for line in contents
        .lines()
        .map(str::trim)
        .map(|line| line.replace('<', "~").replace('>', "~"))
    {
        if line.contains(" operator ") {
            continue;
        }
        if line.contains("DataContext =~ ") {
            data_context = line
                .split_once("DataContext =~ ")
                .map(|(_, after)| after.to_string());
        }
        // class definition detection
        else if (line.starts_with("public ")
            || line.starts_with("private ")
            || line.starts_with("internal ")
            || line.starts_with("protected "))
            && (line.contains("class ") || line.contains("interface "))
            && !needs_close_brace
        {
            class_name = filename.replace(".xaml", "").replace(".cshtml", "");
            if is_interface(&line) {
                file_contents.push_str("\n\t\t~~interface~~");
            }
            let re = Regex::new(r"~[^~]*~").unwrap();
            let parts: Vec<String> = line
                .replace(":", " : ") // Add spaces around colon, just in case
                .split_whitespace()
                .map(|p| p.trim_matches(','))
                .map(|p| re.replace_all(p, "~T~"))
                .map(|p| p.to_string())
                .collect();
            if let Some(index) = parts.iter().position(|s| s == ":") {
                let (_before, after) = parts.split_at(index);
                let base_types = &after[1..];
                for item in base_types {
                    let arrow = Arrow {
                        start: class_name.clone(),
                        end: item.to_owned(),
                        arrow_type: if is_interface(item) {
                            ArrowType::Inheritance
                        } else {
                            ArrowType::Realization
                        },
                    };
                    arrow_list.push(arrow);
                }
            }
            file_contents.push_str(&format!("\n\tclass {} {{", class_name));
            class_list.push(class_name.clone());
            needs_close_brace = true;
            continue;
        }
        // methods
        if let Some(captures) = method_regex.captures(line.as_str()) {
            let visibility = captures.name("visibility").map_or("", |m| m.as_str());
            let return_type = captures.name("return_type").map_or("", |m| m.as_str());
            let name = captures["name"].to_string();
            let params = captures.name("params").map_or("", |m| m.as_str());
            match visibility {
                "private" => file_contents.push_str("\n\t\t- "),
                "protected" => file_contents.push_str("\n\t\t# "),
                _ => file_contents.push_str("\n\t\t+ "),
            }
            let parts: Vec<&str> = params.split(',').map(|s| s.trim()).collect();
            let formatted_params: Vec<String> = parts
                .into_iter()
                .map(|param| {
                    if param.len() == 0 {
                        return format!("");
                    }
                    let mut split = param.split_whitespace();
                    let param_type = split.next().unwrap_or("");
                    let param_name = split.next().unwrap_or("");
                    format!("{}: {}", param_name, param_type)
                })
                .collect();
            file_contents.push_str(&format!(
                "{}({}) {} ",
                name,
                formatted_params.join(", "),
                return_type
            ));
        }
        // enums
        else if let Some(captures) = enum_regex.captures(line.as_str()) {
            let name = captures.name("name").map_or("", |m| m.as_str());
            let values = captures.name("values").map_or("", |m| m.as_str());
            file_contents.push_str(&format!("\n\tclass {} {{\n\t\t<<enumerator>>", name));
            if values.len() > 0 {
                for item in values.trim().split(',').map(|s| s.replace(",", "")) {
                    file_contents.push_str(&format!("\n\t\t{}", item.trim()));
                }
            } else {
                for l in contents
                    .lines()
                    .skip_while(|&l| !l.contains(line.as_str()))
                    .skip(1)
                    .take_while(|&l| !l.trim().contains("}"))
                {
                    let words: Vec<&str> = l
                        .split(',')
                        .map(|w| w.trim())
                        .filter(|w| !w.is_empty())
                        .collect();
                    for word in words {
                        file_contents.push_str(&format!("\n\t\t{}", word.trim()));
                    }
                }
            }
            file_contents.push_str("\n\t}");
        }
        // properties
        else if let Some(captures) = property_regex.captures(line.as_str()) {
            let visibility = captures.name("visibility").map_or("", |m| m.as_str());
            let modifier = captures.name("modifier").map_or("", |m| m.as_str());
            let data_type = captures.name("type").map_or("", |m| m.as_str());
            let name = captures["name"].to_string().replace(";", "");
            match visibility {
                "private" => file_contents.push_str("\n\t\t- "),
                "protected" => file_contents.push_str("\n\t\t# "),
                _ => file_contents.push_str("\n\t\t+ "),
            }
            file_contents.push_str(format!("{}: {}", name, data_type).as_str());
            let arrow = Arrow {
                start: class_name.clone(),
                end: if data_type.contains("~") {
                    let mut parts = data_type.splitn(3, "~");
                    parts.next().unwrap();
                    parts.next().unwrap().to_owned()
                } else {
                    data_type.to_owned()
                },
                arrow_type: if data_type.contains("~") {
                    ArrowType::Composition
                } else {
                    ArrowType::Aggregation
                },
            };
            arrow_list.push(arrow);
            if modifier.contains("event") {
                file_contents.push_str(", ~~event~~");
            }
            if line.contains("get;") {
                file_contents.push_str(", ~~get~~");
            }
            if line.contains("set;") {
                file_contents.push_str(", ~~set~~");
            }
        }
        // getters and setters on other lines
        else {
            if line.starts_with("get") {
                file_contents.push_str(", ~~get~~");
            }
            if line.starts_with("set") {
                file_contents.push_str(", ~~set~~");
            }
        }
    }
    if needs_close_brace {
        file_contents.push_str("\n\t}");
    }
    match data_context {
        Some(value) => {
            let arrow = Arrow {
                start: class_name.clone(),
                end: value.to_owned(),
                arrow_type: ArrowType::Realization,
            };
            arrow_list.push(arrow);
        }
        None => (),
    }
    file_contents
}

fn is_interface(s: &str) -> bool {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some('I'), Some(c)) if c.is_uppercase() => true,
        _ => false,
    }
}
