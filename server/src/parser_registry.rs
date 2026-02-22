//! Parser Registry - Manages all language parsers implementing the CodeParser trait.

use codegraph::CodeGraph;
use codegraph_c::CParser;
use codegraph_cpp::CppParser;
use codegraph_csharp::CSharpParser;
use codegraph_go::GoParser;
use codegraph_java::JavaParser;
use codegraph_kotlin::KotlinParser;
use codegraph_parser_api::{CodeParser, FileInfo, ParserConfig, ParserError, ParserMetrics};
use codegraph_php::PhpParser;
use codegraph_python::PythonParser;
use codegraph_ruby::RubyParser;
use codegraph_rust::RustParser;
use codegraph_swift::SwiftParser;
use codegraph_tcl::TclParser;
use codegraph_typescript::TypeScriptParser;
use std::path::Path;
use std::sync::Arc;

/// Registry of all available language parsers.
pub struct ParserRegistry {
    python: Arc<PythonParser>,
    rust: Arc<RustParser>,
    typescript: Arc<TypeScriptParser>,
    go: Arc<GoParser>,
    c: Arc<CParser>,
    java: Arc<JavaParser>,
    cpp: Arc<CppParser>,
    kotlin: Arc<KotlinParser>,
    csharp: Arc<CSharpParser>,
    php: Arc<PhpParser>,
    ruby: Arc<RubyParser>,
    swift: Arc<SwiftParser>,
    tcl: Arc<TclParser>,
}

impl ParserRegistry {
    /// Create a new parser registry with default configuration.
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Create a new parser registry with custom configuration.
    pub fn with_config(config: ParserConfig) -> Self {
        Self {
            python: Arc::new(PythonParser::with_config(config.clone())),
            rust: Arc::new(RustParser::with_config(config.clone())),
            typescript: Arc::new(TypeScriptParser::with_config(config.clone())),
            go: Arc::new(GoParser::with_config(config.clone())),
            c: Arc::new(CParser::with_config(config.clone())),
            java: Arc::new(JavaParser::with_config(config.clone())),
            cpp: Arc::new(CppParser::with_config(config.clone())),
            kotlin: Arc::new(KotlinParser::with_config(config.clone())),
            csharp: Arc::new(CSharpParser::with_config(config.clone())),
            php: Arc::new(PhpParser::with_config(config.clone())),
            ruby: Arc::new(RubyParser::with_config(config.clone())),
            swift: Arc::new(SwiftParser::with_config(config.clone())),
            tcl: Arc::new(TclParser::with_config(config)),
        }
    }

    /// Get parser by language identifier.
    pub fn get_parser(&self, language: &str) -> Option<Arc<dyn CodeParser>> {
        match language.to_lowercase().as_str() {
            "python" => Some(self.python.clone()),
            "rust" => Some(self.rust.clone()),
            "typescript" | "javascript" | "typescriptreact" | "javascriptreact" => {
                Some(self.typescript.clone())
            }
            "go" => Some(self.go.clone()),
            "c" => Some(self.c.clone()),
            "java" => Some(self.java.clone()),
            "cpp" | "c++" => Some(self.cpp.clone()),
            "kotlin" => Some(self.kotlin.clone()),
            "csharp" | "c#" => Some(self.csharp.clone()),
            "php" => Some(self.php.clone()),
            "ruby" => Some(self.ruby.clone()),
            "swift" => Some(self.swift.clone()),
            "tcl" => Some(self.tcl.clone()),
            _ => None,
        }
    }

    /// Find appropriate parser for a file path.
    ///
    /// Note: C is checked before C++ so `.h` files default to C parsing.
    /// C++-specific extensions (`.hpp`, `.cc`, `.cxx`, `.hh`, `.hxx`) are
    /// only claimed by the C++ parser and resolve correctly.
    pub fn parser_for_path(&self, path: &Path) -> Option<Arc<dyn CodeParser>> {
        let parsers: [Arc<dyn CodeParser>; 13] = [
            self.python.clone(),
            self.rust.clone(),
            self.typescript.clone(),
            self.go.clone(),
            self.c.clone(),
            self.java.clone(),
            self.cpp.clone(),
            self.kotlin.clone(),
            self.csharp.clone(),
            self.php.clone(),
            self.ruby.clone(),
            self.swift.clone(),
            self.tcl.clone(),
        ];

        parsers.into_iter().find(|p| p.can_parse(path))
    }

    /// Get all supported file extensions.
    pub fn supported_extensions(&self) -> Vec<&str> {
        let mut extensions = Vec::new();
        extensions.extend(self.python.file_extensions().iter().copied());
        extensions.extend(self.rust.file_extensions().iter().copied());
        extensions.extend(self.typescript.file_extensions().iter().copied());
        extensions.extend(self.go.file_extensions().iter().copied());
        extensions.extend(self.c.file_extensions().iter().copied());
        extensions.extend(self.java.file_extensions().iter().copied());
        extensions.extend(self.cpp.file_extensions().iter().copied());
        extensions.extend(self.kotlin.file_extensions().iter().copied());
        extensions.extend(self.csharp.file_extensions().iter().copied());
        extensions.extend(self.php.file_extensions().iter().copied());
        extensions.extend(self.ruby.file_extensions().iter().copied());
        extensions.extend(self.swift.file_extensions().iter().copied());
        extensions.extend(self.tcl.file_extensions().iter().copied());
        extensions
    }

    /// Get metrics from all parsers.
    pub fn all_metrics(&self) -> Vec<(&str, ParserMetrics)> {
        vec![
            ("python", self.python.metrics()),
            ("rust", self.rust.metrics()),
            ("typescript", self.typescript.metrics()),
            ("go", self.go.metrics()),
            ("c", self.c.metrics()),
            ("java", self.java.metrics()),
            ("cpp", self.cpp.metrics()),
            ("kotlin", self.kotlin.metrics()),
            ("csharp", self.csharp.metrics()),
            ("php", self.php.metrics()),
            ("ruby", self.ruby.metrics()),
            ("swift", self.swift.metrics()),
            ("tcl", self.tcl.metrics()),
        ]
    }

    /// Check if a file path is supported by any parser.
    pub fn can_parse(&self, path: &Path) -> bool {
        self.parser_for_path(path).is_some()
    }

    /// Parse a file using the appropriate parser.
    pub fn parse_file(&self, path: &Path, graph: &mut CodeGraph) -> Result<FileInfo, ParserError> {
        let parser = self.parser_for_path(path).ok_or_else(|| {
            ParserError::UnsupportedFeature(path.to_path_buf(), "Unsupported file type".to_string())
        })?;

        parser.parse_file(path, graph)
    }

    /// Parse source code string using the appropriate parser for the given path.
    pub fn parse_source(
        &self,
        source: &str,
        path: &Path,
        graph: &mut CodeGraph,
    ) -> Result<FileInfo, ParserError> {
        let parser = self.parser_for_path(path).ok_or_else(|| {
            ParserError::UnsupportedFeature(path.to_path_buf(), "Unsupported file type".to_string())
        })?;

        parser.parse_source(source, path, graph)
    }

    /// Get language name for a file path.
    ///
    /// Note: `.h` files return `"c"` by convention (C-compatible headers).
    /// Use `.hpp`/`.hh`/`.hxx` for C++ headers.
    pub fn language_for_path(&self, path: &Path) -> Option<&'static str> {
        if self.python.can_parse(path) {
            Some("python")
        } else if self.rust.can_parse(path) {
            Some("rust")
        } else if self.typescript.can_parse(path) {
            // Determine if it's TypeScript or JavaScript
            if let Some(ext) = path.extension() {
                match ext.to_str() {
                    Some("ts") | Some("tsx") => Some("typescript"),
                    Some("js") | Some("jsx") => Some("javascript"),
                    _ => Some("typescript"),
                }
            } else {
                Some("typescript")
            }
        } else if self.go.can_parse(path) {
            Some("go")
        } else if self.c.can_parse(path) {
            Some("c")
        } else if self.java.can_parse(path) {
            Some("java")
        } else if self.cpp.can_parse(path) {
            Some("cpp")
        } else if self.kotlin.can_parse(path) {
            Some("kotlin")
        } else if self.csharp.can_parse(path) {
            Some("csharp")
        } else if self.php.can_parse(path) {
            Some("php")
        } else if self.ruby.can_parse(path) {
            Some("ruby")
        } else if self.swift.can_parse(path) {
            Some("swift")
        } else if self.tcl.can_parse(path) {
            Some("tcl")
        } else {
            None
        }
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parser_registry_new() {
        let registry = ParserRegistry::new();
        assert!(registry.get_parser("python").is_some());
        assert!(registry.get_parser("rust").is_some());
        assert!(registry.get_parser("typescript").is_some());
        assert!(registry.get_parser("go").is_some());
        assert!(registry.get_parser("c").is_some());
        assert!(registry.get_parser("java").is_some());
        assert!(registry.get_parser("cpp").is_some());
        assert!(registry.get_parser("kotlin").is_some());
        assert!(registry.get_parser("csharp").is_some());
        assert!(registry.get_parser("php").is_some());
        assert!(registry.get_parser("ruby").is_some());
        assert!(registry.get_parser("swift").is_some());
        assert!(registry.get_parser("tcl").is_some());
    }

    #[test]
    fn test_parser_registry_default() {
        let registry = ParserRegistry::default();
        // Should be equivalent to new()
        assert!(registry.get_parser("python").is_some());
    }

    #[test]
    fn test_parser_registry_with_config() {
        let config = ParserConfig::default();
        let registry = ParserRegistry::with_config(config);
        assert!(registry.get_parser("python").is_some());
    }

    #[test]
    fn test_get_parser_case_insensitive() {
        let registry = ParserRegistry::new();

        assert!(registry.get_parser("Python").is_some());
        assert!(registry.get_parser("PYTHON").is_some());
        assert!(registry.get_parser("Rust").is_some());
        assert!(registry.get_parser("RUST").is_some());
        assert!(registry.get_parser("TypeScript").is_some());
        assert!(registry.get_parser("Go").is_some());
        assert!(registry.get_parser("C").is_some());
        assert!(registry.get_parser("Java").is_some());
        assert!(registry.get_parser("JAVA").is_some());
        assert!(registry.get_parser("Cpp").is_some());
        assert!(registry.get_parser("C++").is_some());
        assert!(registry.get_parser("Kotlin").is_some());
        assert!(registry.get_parser("CSharp").is_some());
        assert!(registry.get_parser("C#").is_some());
        assert!(registry.get_parser("PHP").is_some());
        assert!(registry.get_parser("Ruby").is_some());
        assert!(registry.get_parser("Swift").is_some());
        assert!(registry.get_parser("TCL").is_some());
    }

    #[test]
    fn test_get_parser_javascript_variants() {
        let registry = ParserRegistry::new();

        // All JS variants should return the typescript parser
        assert!(registry.get_parser("javascript").is_some());
        assert!(registry.get_parser("typescriptreact").is_some());
        assert!(registry.get_parser("javascriptreact").is_some());
    }

    #[test]
    fn test_get_parser_unknown_language() {
        let registry = ParserRegistry::new();

        assert!(registry.get_parser("cobol").is_none());
        assert!(registry.get_parser("unknown").is_none());
        assert!(registry.get_parser("").is_none());
    }

    #[test]
    fn test_parser_for_path() {
        let registry = ParserRegistry::new();

        assert!(registry
            .parser_for_path(&PathBuf::from("test.py"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.rs"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.ts"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.js"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.go"))
            .is_some());
        assert!(registry.parser_for_path(&PathBuf::from("test.c")).is_some());
        assert!(registry.parser_for_path(&PathBuf::from("test.h")).is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.java"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.cpp"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.kt"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.cs"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.php"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.rb"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.swift"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.tcl"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.txt"))
            .is_none());
    }

    #[test]
    fn test_parser_for_path_php_variants() {
        let registry = ParserRegistry::new();

        // PHP parser only supports .php extension
        assert!(registry
            .parser_for_path(&PathBuf::from("index.php"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("artisan.php"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("template.phtml"))
            .is_none());
    }

    #[test]
    fn test_parser_for_path_ruby_variants() {
        let registry = ParserRegistry::new();

        // Ruby parser supports .rb, .rake, .gemspec
        assert!(registry.parser_for_path(&PathBuf::from("app.rb")).is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("Rakefile.rake"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("my_gem.gemspec"))
            .is_some());
        // Bare Rakefile/Gemfile (no extension) are not supported
        assert!(registry
            .parser_for_path(&PathBuf::from("Rakefile"))
            .is_none());
    }

    #[test]
    fn test_parser_for_path_cpp_variants() {
        let registry = ParserRegistry::new();

        assert!(registry
            .parser_for_path(&PathBuf::from("test.cc"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.cxx"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.hpp"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.hh"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("test.hxx"))
            .is_some());
    }

    #[test]
    fn test_parser_for_path_kotlin_script() {
        let registry = ParserRegistry::new();

        assert!(registry
            .parser_for_path(&PathBuf::from("build.gradle.kts"))
            .is_some());
    }

    #[test]
    fn test_parser_for_path_react_extensions() {
        let registry = ParserRegistry::new();

        assert!(registry
            .parser_for_path(&PathBuf::from("component.tsx"))
            .is_some());
        assert!(registry
            .parser_for_path(&PathBuf::from("component.jsx"))
            .is_some());
    }

    #[test]
    fn test_language_for_path() {
        let registry = ParserRegistry::new();

        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.py")),
            Some("python")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.rs")),
            Some("rust")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.ts")),
            Some("typescript")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.js")),
            Some("javascript")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.go")),
            Some("go")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.c")),
            Some("c")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.h")),
            Some("c")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("Test.java")),
            Some("java")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.cpp")),
            Some("cpp")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.kt")),
            Some("kotlin")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.cs")),
            Some("csharp")
        );
    }

    #[test]
    fn test_language_for_path_new_parsers() {
        let registry = ParserRegistry::new();

        assert_eq!(
            registry.language_for_path(&PathBuf::from("index.php")),
            Some("php")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("app.rb")),
            Some("ruby")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("ContentView.swift")),
            Some("swift")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("script.tcl")),
            Some("tcl")
        );
    }

    #[test]
    fn test_language_for_path_cpp_variants() {
        let registry = ParserRegistry::new();

        // C++-specific extensions all resolve to "cpp"
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.cpp")),
            Some("cpp")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.cc")),
            Some("cpp")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.cxx")),
            Some("cpp")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.hpp")),
            Some("cpp")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.hh")),
            Some("cpp")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.hxx")),
            Some("cpp")
        );

        // .h defaults to C (conventional: .h is C-compatible, .hpp signals C++)
        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.h")),
            Some("c")
        );
    }

    #[test]
    fn test_language_for_path_kotlin_variants() {
        let registry = ParserRegistry::new();

        assert_eq!(
            registry.language_for_path(&PathBuf::from("test.kt")),
            Some("kotlin")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("build.gradle.kts")),
            Some("kotlin")
        );
    }

    #[test]
    fn test_language_for_path_react_extensions() {
        let registry = ParserRegistry::new();

        assert_eq!(
            registry.language_for_path(&PathBuf::from("component.tsx")),
            Some("typescript")
        );
        assert_eq!(
            registry.language_for_path(&PathBuf::from("component.jsx")),
            Some("javascript")
        );
    }

    #[test]
    fn test_language_for_path_unknown() {
        let registry = ParserRegistry::new();

        assert_eq!(registry.language_for_path(&PathBuf::from("test.txt")), None);
        assert_eq!(registry.language_for_path(&PathBuf::from("Makefile")), None);
    }

    #[test]
    fn test_supported_extensions() {
        let registry = ParserRegistry::new();
        let extensions = registry.supported_extensions();

        // Check that we have extensions for all 13 languages
        // (exact extension names may vary by parser implementation)
        assert!(!extensions.is_empty());
        assert!(extensions.len() >= 13); // At least one extension per language
    }

    #[test]
    fn test_can_parse() {
        let registry = ParserRegistry::new();

        assert!(registry.can_parse(Path::new("test.py")));
        assert!(registry.can_parse(Path::new("test.rs")));
        assert!(registry.can_parse(Path::new("test.ts")));
        assert!(registry.can_parse(Path::new("test.js")));
        assert!(registry.can_parse(Path::new("test.go")));
        assert!(registry.can_parse(Path::new("test.c")));
        assert!(registry.can_parse(Path::new("test.h")));
        assert!(registry.can_parse(Path::new("test.java")));
        assert!(registry.can_parse(Path::new("test.cpp")));
        assert!(registry.can_parse(Path::new("test.hpp")));
        assert!(registry.can_parse(Path::new("test.cc")));
        assert!(registry.can_parse(Path::new("test.kt")));
        assert!(registry.can_parse(Path::new("test.kts")));
        assert!(registry.can_parse(Path::new("test.cs")));
        assert!(registry.can_parse(Path::new("test.php")));
        assert!(registry.can_parse(Path::new("test.rb")));
        assert!(registry.can_parse(Path::new("test.swift")));
        assert!(registry.can_parse(Path::new("test.tcl")));
        assert!(!registry.can_parse(Path::new("test.txt")));
        assert!(!registry.can_parse(Path::new("test.md")));
    }

    #[test]
    fn test_all_metrics() {
        let registry = ParserRegistry::new();
        let metrics = registry.all_metrics();

        assert_eq!(metrics.len(), 13);
        assert_eq!(metrics[0].0, "python");
        assert_eq!(metrics[1].0, "rust");
        assert_eq!(metrics[2].0, "typescript");
        assert_eq!(metrics[3].0, "go");
        assert_eq!(metrics[4].0, "c");
        assert_eq!(metrics[5].0, "java");
        assert_eq!(metrics[6].0, "cpp");
        assert_eq!(metrics[7].0, "kotlin");
        assert_eq!(metrics[8].0, "csharp");
        assert_eq!(metrics[9].0, "php");
        assert_eq!(metrics[10].0, "ruby");
        assert_eq!(metrics[11].0, "swift");
        assert_eq!(metrics[12].0, "tcl");
    }

    #[test]
    fn test_parse_source_python() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "def hello():\n    print('hello')\n";
        let path = Path::new("test.py");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_rust() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "fn hello() { println!(\"hello\"); }";
        let path = Path::new("test.rs");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_typescript() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "function hello(): void { console.log('hello'); }";
        let path = Path::new("test.ts");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_go() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "package main\n\nfunc hello() { fmt.Println(\"hello\") }";
        let path = Path::new("test.go");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_c() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "#include <stdio.h>\n\nvoid hello() { printf(\"hello\\n\"); }";
        let path = Path::new("test.c");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_unsupported() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "some content";
        let path = Path::new("test.txt");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_file() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();

        // Create a temporary Python file
        let mut temp_file = NamedTempFile::with_suffix(".py").unwrap();
        writeln!(temp_file, "def test_function():\n    pass").unwrap();

        let result = registry.parse_file(temp_file.path(), &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_file_unsupported() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();

        // Create a temporary text file
        let mut temp_file = NamedTempFile::with_suffix(".txt").unwrap();
        writeln!(temp_file, "some text content").unwrap();

        let result = registry.parse_file(temp_file.path(), &mut graph);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_source_java() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source =
            "public class Hello { public void greet() { System.out.println(\"hello\"); } }";
        let path = Path::new("Hello.java");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_cpp() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "#include <iostream>\n\nvoid hello() { std::cout << \"hello\" << std::endl; }";
        let path = Path::new("hello.cpp");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_kotlin() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "fun main() { println(\"hello\") }";
        let path = Path::new("Main.kt");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_csharp() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source =
            "using System;\n\nclass Hello { static void Main() { Console.WriteLine(\"hello\"); } }";
        let path = Path::new("Hello.cs");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_php() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "<?php\nfunction hello(): void {\n    echo 'hello';\n}\n";
        let path = Path::new("hello.php");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_php_class() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "<?php\nnamespace App\\Http\\Controllers;\n\nclass UserController {\n    public function index(): array {\n        return [];\n    }\n}\n";
        let path = Path::new("UserController.php");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_ruby() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "def hello\n  puts 'hello'\nend\n";
        let path = Path::new("hello.rb");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_ruby_class() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "class Greeter\n  def initialize(name)\n    @name = name\n  end\n\n  def greet\n    puts \"Hello, #{@name}!\"\n  end\nend\n";
        let path = Path::new("greeter.rb");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_swift() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "func hello() {\n    print(\"hello\")\n}\n";
        let path = Path::new("hello.swift");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_swift_class() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "import Foundation\n\nclass Greeter {\n    let name: String\n\n    init(name: String) {\n        self.name = name\n    }\n\n    func greet() -> String {\n        return \"Hello, \\(name)!\"\n    }\n}\n";
        let path = Path::new("Greeter.swift");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_tcl() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "proc hello {} {\n    puts \"hello\"\n}\n";
        let path = Path::new("hello.tcl");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_source_tcl_namespace() {
        let registry = ParserRegistry::new();
        let mut graph = CodeGraph::in_memory().unwrap();
        let source = "namespace eval Greeter {\n    proc new {name} {\n        return $name\n    }\n    proc greet {name} {\n        puts \"Hello, $name!\"\n    }\n}\n";
        let path = Path::new("greeter.tcl");

        let result = registry.parse_source(source, path, &mut graph);
        assert!(result.is_ok());
    }
}
