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
        let mut file = File::create(&umlpath).expect("uml.mmd could not be created");
        file.write_all(b"classDiagram\n\tdirection UD")
            .expect("error writing to file");

        for entry in WalkDir::new(current_dir) {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("cs") {
                println!("parsing file: {}", path.display());
                let contents = fs::read_to_string(path).expect("file read error");
                parse_contents(contents, &mut file);
            }
        }
    } else {
        println!("no directory to search");
    }
    println!("uml.mmd created");
}

fn parse_contents(contents: String, file: &mut File) {
    let mut need_closing_brace = false;
    for line in contents
        .lines()
        .map(str::trim)
        .map(|line| line.replace('<', "~").replace('>', "~"))
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
            parse_class(line, file);
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
            parse_method(line, file);
        }
        // Enums
        else if (line.starts_with("public")
            || line.starts_with("private")
            || line.starts_with("protected"))
            && line.contains(" enum ")
        {
            parse_enum(line, &contents, file);
        }
        // Property / global var definition
        else if line.starts_with("public")
            || line.starts_with("private")
            || line.starts_with("protected")
        {
            parse_attribute(line, file);
        }
        // Getters and setters on different lines
        else if line.starts_with("get") {
            write!(file, " [get]").expect("error writing to file");
        } else if line.starts_with("set") {
            write!(file, " [set]").expect("error writing to file");
        }
    }
    if need_closing_brace {
        write!(file, "\n\t}}").expect("error writing to file");
    }
}

fn parse_class(line: String, file: &mut File) {
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
                write!(file, "\n\t{} ..* {}", class_name, item).expect("error writing to file");
            } else {
                write!(file, "\n\t{} --* {}", class_name, item).expect("error writing to file");
            }
        }
    } else {
        class_name = parts.last().unwrap();
    }
    write!(file, "\n\tclass {} {{", class_name).expect("error writing to file");
    if is_interface(class_name) {
        write!(file, "\n\t\t<<interface>>").expect("error writing to file");
    }
}

fn parse_method(line: String, file: &mut File) {
    let re = Regex::new(r"(?P<visibility>public|private|protected)?\s*(?P<modifiers>static|virtual|abstract|override|async|sealed|extern|unsafe|partial|readonly)?\s*(?P<return_type>[^\s]+)?\s+(?P<name>[^\s]+)\((?P<params>[^\)]*)\)").unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        let visibility = caps.name("visibility").map_or("", |m| m.as_str());
        // modifiers is unused currently
        let _modifiers = caps.name("modifiers").map_or("", |m| m.as_str()).trim();
        let return_type = caps.name("return_type").map_or("", |m| m.as_str());
        let name = caps.name("name").map_or("", |m| m.as_str());
        let params = caps.name("params").map_or("", |m| m.as_str());
        match visibility {
            "private" => write!(file, "\t\t- ").expect("error writing to file"),
            "protected" => write!(file, "\t\t# ").expect("error writing to file"),
            _ => write!(file, "\t\t+ ").expect("error writing to file"),
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
        write!(
            file,
            "{}({}) {}",
            name,
            formatted_params.join(", "),
            return_type
        )
        .expect("error writing to file");
    }
}

fn parse_enum(line: String, contents: &String, file: &mut File) {
    let re = Regex::new(r"enum\s+(?P<name>\w+)\s*\{(?P<values>[^}]*)\}?").unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        let name = caps.name("name").map_or("", |m| m.as_str());
        let values = caps.name("values").map_or("", |m| m.as_str());
        write!(file, "\n\tclass {} {{\n\t\t<<enumerator>>", name).expect("error writing to file");
        if values.len() > 0 {
            for item in values.trim().split(',').map(|s| s.replace(",", "")) {
                write!(file, "\n\t\t{}", item.trim()).expect("error writing to file");
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
                    write!(file, "\n\t\t{}", word.trim()).expect("error writing to file");
                }
            }
        }
    }
    write!(file, "\n\t}}").expect("error writing to file");
}

fn parse_attribute(line: String, file: &mut File) {
    let re = Regex::new(r"(?P<visibility>public|private|protected)?\s*(?P<modifiers>static|virtual|abstract|override|async|sealed|extern|unsafe|partial|readonly)?\s*(?P<type>[^\s]+)?\s+(?P<name>[^\s]+)").unwrap();
    if let Some(caps) = re.captures(line.as_str()) {
        let visibility = caps.name("visibility").map_or("", |m| m.as_str());
        // modifiers is unused currently
        let _modifiers = caps.name("modifiers").map_or("", |m| m.as_str()).trim();
        let var_type = caps.name("type").map_or("", |m| m.as_str());
        let name = caps.name("name").map_or("", |m| m.as_str());
        // Handle enums
        if var_type == "enum" {
            return;
        }
        match visibility {
            "private" => write!(file, "\n\t\t- ").expect("error writing to file"),
            "protected" => write!(file, "\n\t\t# ").expect("error writing to file"),
            _ => write!(file, "\n\t\t+ ").expect("error writing to file"),
        }
        write!(file, "{}: {}", name, var_type).expect("error writing to file");
    }
    if line.contains("get;") {
        write!(file, " [get]").expect("error writing to file");
    }
    if line.contains("set;") {
        write!(file, " [set]").expect("error writing to file");
    }
}

fn is_interface(s: &str) -> bool {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some('I'), Some(c)) if c.is_uppercase() => true,
        _ => false,
    }
}
