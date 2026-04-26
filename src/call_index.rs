//! Local call index - stores which functions call which.
//! Built from tree-sitter parsed call sites. No database.
//! Persisted at ~/.savants/calls/{repo}.json

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct CallIndex {
    /// function_name -> list of (caller_name, caller_file, caller_line)
    pub callers: HashMap<String, Vec<CallRef>>,
    /// function_name -> list of functions it calls
    pub callees: HashMap<String, Vec<String>>,
    /// function_name -> list of files that import it
    pub importers: HashMap<String, Vec<String>>,
    /// All functions with their files
    pub functions: Vec<FuncRef>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CallRef {
    pub name: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FuncRef {
    pub name: String,
    pub file: String,
    pub line: u32,
}

impl CallIndex {
    pub fn from_parse_result(result: &crate::code_parser::ParseResult) -> Self {
        let mut callers: HashMap<String, Vec<CallRef>> = HashMap::new();
        let mut callees: HashMap<String, Vec<String>> = HashMap::new();
        let mut importers: HashMap<String, Vec<String>> = HashMap::new();
        let mut functions = vec![];

        // Build function list
        for entity in &result.entities {
            if entity.kind == "function" {
                functions.push(FuncRef {
                    name: entity.name.clone(),
                    file: entity.file.clone(),
                    line: entity.line as u32,
                });
            }
        }

        // Build caller/callee maps from call sites
        let func_names: std::collections::HashSet<String> = functions.iter()
            .map(|f| f.name.clone()).collect();

        for cs in &result.call_sites {
            if func_names.contains(&cs.callee_name) {
                callers.entry(cs.callee_name.clone()).or_default().push(CallRef {
                    name: cs.caller_name.clone(),
                    file: cs.caller_file.clone(),
                    line: 0, // we don't track exact call line in parse result
                });
                callees.entry(cs.caller_name.clone()).or_default().push(cs.callee_name.clone());
            }
        }

        // Build import map
        for entity in &result.entities {
            if entity.kind == "import" {
                for imported_name in &entity.import_names {
                    importers.entry(imported_name.clone()).or_default().push(entity.file.clone());
                }
            }
        }

        CallIndex { callers, callees, importers, functions }
    }

    pub fn save(&self, repo: &str) -> Result<(), String> {
        let path = store_path(repo);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn load(repo: &str) -> Result<Self, String> {
        let path = store_path(repo);
        let data = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }

    pub fn exists(repo: &str) -> bool {
        store_path(repo).exists()
    }

    /// Find all callers of a function.
    pub fn find_callers(&self, function: &str) -> Vec<&CallRef> {
        self.callers.get(function).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Find all files that import a function.
    pub fn find_importers(&self, function: &str) -> Vec<&String> {
        self.importers.get(function).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Find all usages: callers + importers + body references.
    pub fn find_where_used(&self, symbol: &str) -> (Vec<&CallRef>, Vec<&String>) {
        let callers = self.find_callers(symbol);
        let importers = self.find_importers(symbol);
        (callers, importers)
    }
}

fn store_path(repo: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".savants")
        .join("calls")
        .join(format!("{}.json", repo))
}
