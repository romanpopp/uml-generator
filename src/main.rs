use regex::Regex;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::{env, fs};
use walkdir::WalkDir;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let current_dir = &args[1];
        let umlpath = Path::new(current_dir).join("uml.mmd");
        let mut file_contents = String::new();
        let mut classes = Vec::new();
        let mut potential_arrows: Vec<(String, String)> = Vec::new();
        file_contents.push_str("classDiagram\n\tdirection UD");
        for entry in WalkDir::new(current_dir) {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("cs")
                || path.extension().and_then(|s| s.to_str()) == Some("xaml")
            {
                println!("parsing file: {}", path.display());
                let contents = fs::read_to_string(path).expect("file read error");
                file_contents.push_str(
                    parse_contents(contents, &mut classes, &mut potential_arrows).as_str(),
                );
            }
        }
        for arrow in potential_arrows {
            if classes.contains(&arrow.0) {
                file_contents
                    .push_str(format!("\n\t{} \"1\" --* \"1\" {}", arrow.0, arrow.1).as_str());
            } else {
                if let Some(class) = classes
                    .iter()
                    .find(|c| arrow.0.contains(format!("~{}~", c).as_str()))
                {
                    file_contents
                        .push_str(format!("\n\t{} \"0..*\" --o \"1\" {}", class, arrow.1).as_str());
                }
            }
        }
        let modified_contents = file_contents.replace("~~get~~, ~~set~~", "~~get + set~~");
        let mut file = File::create(&umlpath).expect("error creating uml.mmd file");
        file.write_all(modified_contents.as_bytes())
            .expect("error writing contents to uml.mmd file");
    } else {
        println!("no directory to search");
    }
    println!("uml.mmd created");
}

fn parse_contents(
    contents: String,
    classes: &mut Vec<String>,
    potential_arrows: &mut Vec<(String, String)>,
) -> String {
    let mut file_contents = String::new();
    let mut class_name: String = String::new();
    let mut xaml_class_name: String = String::new();
    let mut need_closing_brace = false;

    for line in contents
        .lines()
        .map(str::trim)
        .map(|line| line.replace('<', "~").replace('>', "~"))
        .map(|line| line.replace(';', ""))
    {
        // Class / interface definition
        if (line.starts_with("public")
            || line.starts_with("private")
            || line.starts_with("internal")
            || line.starts_with("protected"))
            && (line.contains("class") || line.contains("interface"))
            && !need_closing_brace
            && !line.contains(";")
        {
            need_closing_brace = true;
            file_contents.push_str(parse_class(line, classes).as_str());
            class_name = classes.last().expect("yeowza").to_string();
        }
        // Method definition
        else if (line.starts_with("public")
            || line.starts_with("private")
            || line.starts_with("internal")
            || line.starts_with("protected"))
            && (line.contains("(") || line.contains(")"))
            && !line.contains(" new ")
            && !line.contains("=")
        {
            file_contents.push_str(parse_method(line).as_str());
        }
        // Enums
        else if (line.starts_with("public")
            || line.starts_with("private")
            || line.starts_with("protected"))
            && line.contains(" enum ")
        {
            file_contents.push_str(parse_enum(line, &contents).as_str());
        }
        // Property / global var definition
        else if line.starts_with("public")
            || line.starts_with("private")
            || line.starts_with("protected")
        {
            let data = parse_attribute(line);
            potential_arrows.push((data.1, class_name.clone()));
            file_contents.push_str(data.0.as_str());
        }
        // Getters and setters on different lines
        else if line.starts_with("get") {
            file_contents.push_str(", ~~get~~")
        } else if line.starts_with("set") {
            file_contents.push_str(", ~~set~~")
        } else if line.contains(" x:Name") {
            let data = parse_xaml(line, xaml_class_name.clone());
            potential_arrows.push((data.1, xaml_class_name.clone()));
            file_contents.push_str(data.0.as_str());
        } else if line.contains(" x:Class") {
            xaml_class_name = parse_xaml_class_name(line);
        }
    }

    if need_closing_brace {
        file_contents.push_str("\n\t}");
    }

    file_contents
}

fn parse_class(line: String, classes: &mut Vec<String>) -> String {
    let mut class_contents = String::new();
    let re = Regex::new(r"~[^~]*~").unwrap();
    let parts: Vec<String> = line
        .replace(":", " : ") // Add spaces around colon, just in case
        .split_whitespace()
        .map(|p| p.trim_matches(','))
        .map(|p| re.replace_all(p, "~T~"))
        .map(|p| p.to_string())
        .collect();
    let class_name: &str;
    if let Some(index) = parts.iter().position(|s| s == ":") {
        let (before, after) = parts.split_at(index);
        let definition = &before[..index];
        let base_types = &after[1..];
        class_name = definition.last().unwrap();
        for item in base_types {
            if is_interface(item) {
                class_contents.push_str(&format!("\n\t{} ..|> {}", class_name, item));
            } else {
                class_contents.push_str(&format!("\n\t{} --|> {}", class_name, item))
            }
        }
    } else {
        class_name = parts.last().unwrap();
    }
    class_contents.push_str(&format!("\n\tclass {} {{", class_name));
    if is_interface(class_name) {
        class_contents.push_str("\n\t\t<<interface>>");
    }
    classes.push(class_name.to_string());
    class_contents
}

fn parse_method(line: String) -> String {
    let mut method_contents = String::new();
    let re = Regex::new(r"(?P<visibility>public|private|protected)?\s*(?P<modifiers>static|virtual|abstract|override|async|sealed|extern|unsafe|partial|readonly)?\s*(?P<return_type>[^\s]+)?\s+(?P<name>[^\s]+)\((?P<params>[^\)]*)\)").unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        let visibility = caps.name("visibility").map_or("", |m| m.as_str());
        // modifiers is unused currently
        let _modifiers = caps.name("modifiers").map_or("", |m| m.as_str()).trim();
        let return_type = caps.name("return_type").map_or("", |m| m.as_str());
        let name = caps.name("name").map_or("", |m| m.as_str());
        let params = caps.name("params").map_or("", |m| m.as_str());
        match visibility {
            "private" => method_contents.push_str("\n\t\t- "),
            "protected" => method_contents.push_str("\n\t\t# "),
            _ => method_contents.push_str("\n\t\t+ "),
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
        method_contents.push_str(&format!(
            "{}({}) {}",
            name,
            formatted_params.join(", "),
            return_type
        ));
    }
    method_contents
}

fn parse_enum(line: String, contents: &String) -> String {
    let mut enum_contents = String::new();
    let re = Regex::new(r"enum\s+(?P<name>\w+)\s*\{(?P<values>[^}]*)\}?").unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        let name = caps.name("name").map_or("", |m| m.as_str());
        let values = caps.name("values").map_or("", |m| m.as_str());
        enum_contents.push_str(&format!("\n\tclass {} {{\n\t\t<<enumerator>>", name));
        if values.len() > 0 {
            for item in values.trim().split(',').map(|s| s.replace(",", "")) {
                enum_contents.push_str(&format!("\n\t\t{}", item.trim()));
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
                    enum_contents.push_str(&format!("\n\t\t{}", word.trim()));
                }
            }
        }
    }
    enum_contents.push_str("\n\t}");
    enum_contents
}

fn parse_attribute(line: String) -> (String, String) {
    let mut attribute_contents = String::new();
    let mut var_type = String::new();
    let re = Regex::new(r"(?P<visibility>public|private|protected)?\s*(?P<modifier>static|virtual|abstract|override|async|sealed|extern|unsafe|partial|readonly|event)?\s*(?P<type>[^\s]+)?\s+(?P<name>[^\s]+)").unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        let visibility = caps.name("visibility").map_or("", |m| m.as_str());
        // modifiers is unused currently
        let modifier = caps.name("modifier").map_or("", |m| m.as_str()).trim();
        var_type = caps.name("type").map_or("", |m| m.as_str()).to_string();
        let name = caps.name("name").map_or("", |m| m.as_str());
        match visibility {
            "private" => attribute_contents.push_str("\n\t\t- "),
            "protected" => attribute_contents.push_str("\n\t\t# "),
            _ => attribute_contents.push_str("\n\t\t+ "),
        }
        attribute_contents.push_str(&format!("{}: {}", name, var_type));
        if modifier == "event" {
            attribute_contents.push_str(", ~~event~~");
        }
    }
    if line.contains("get;") {
        attribute_contents.push_str(", ~~get~~");
    }
    if line.contains("set;") {
        attribute_contents.push_str(", ~~set~~");
    }
    (attribute_contents, var_type)
}

fn parse_xaml(line: String, class: String) -> (String, String) {
    let mut contents = String::new();
    let pattern = r#"~(?P<type>\w+)\s+x:Name="(?P<name>[^"]+)""#;
    let re = Regex::new(pattern).unwrap();
    let mut var_type = String::new();
    if let Some(caps) = re.captures(line.as_str()) {
        var_type = caps.name("type").map_or("", |m| m.as_str()).to_string();
        let name = caps.name("name").map_or("", |m| m.as_str());
        contents.push_str(&format!("\n\t{}: + {} {}", class, name, var_type));
    }
    (contents, var_type)
}

fn parse_xaml_class_name(line: String) -> String {
    let pattern = r#"x:Class="(?P<name>[^"]+)""#;
    let re = Regex::new(pattern).unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        return caps
            .name("name")
            .map_or("", |m| m.as_str())
            .rsplit('.')
            .next()
            .unwrap()
            .to_string();
    }
    "".to_string()
}

fn is_interface(s: &str) -> bool {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some('I'), Some(c)) if c.is_uppercase() => true,
        _ => false,
    }
}
