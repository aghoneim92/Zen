use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use zen_diagnostics::SourceMap;
use zen_format::FormatError;
pub fn run(args: &[String]) -> ExitCode {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: zen fmt [--check] <path>...\n       zen fmt -\n\nExit codes: 0 formatted, 1 would change, 2 syntax/formatter/I/O error."
        );
        return ExitCode::SUCCESS;
    }
    match execute(args) {
        Ok(changed) => ExitCode::from(u8::from(changed)),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
fn format(path: &str, source: &str) -> Result<String, String> {
    let mut sources = SourceMap::default();
    let id = sources.add(path, source);
    zen_format::format_file(id, source).map_err(|e| match e {
        FormatError::Syntax(ds) => ds
            .iter()
            .map(|d| d.render(&sources))
            .collect::<Vec<_>>()
            .join("\n"),
        FormatError::Internal(e) => format!("error[ZEN-FMT-0001]: {path}: {e}"),
    })
}
fn execute(args: &[String]) -> Result<bool, String> {
    let check = args.iter().any(|s| s == "--check");
    let paths: Vec<_> = args.iter().filter(|s| s.as_str() != "--check").collect();
    if paths.is_empty()
        || paths
            .iter()
            .any(|s| s.starts_with('-') && s.as_str() != "-")
    {
        return Err("error: expected `zen fmt [--check] <path>...` or `zen fmt -`".into());
    }
    if paths.iter().any(|s| s.as_str() == "-") {
        if paths.len() != 1 || check {
            return Err("error: stdin mode requires exactly `zen fmt -`".into());
        }
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("error[ZEN-IO-0001]: stdin: {e}"))?;
        let output = format("<stdin>", &source)?;
        io::stdout()
            .write_all(output.as_bytes())
            .map_err(|e| format!("error[ZEN-IO-0001]: stdout: {e}"))?;
        return Ok(false);
    }
    let mut files = BTreeSet::new();
    for path in paths {
        discover(Path::new(path), &mut files)
            .map_err(|e| format!("error[ZEN-IO-0001]: {path}: {e}"))?;
    }
    let mut changed = false;
    let mut errors = vec![];
    for path in files {
        let result = (|| {
            let source =
                fs::read_to_string(&path).map_err(|e| format!("error[ZEN-IO-0001]: {e}"))?;
            let output = format(&path.display().to_string(), &source)?;
            if source != output {
                if check {
                    println!("Would format {}", path.display());
                    changed = true;
                } else {
                    atomic_write(&path, &source, &output)
                        .map_err(|e| format!("error[ZEN-IO-0001]: {e}"))?;
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(e) = result {
            errors.push(format!("{}: {e}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(changed)
    } else {
        Err(errors.join("\n"))
    }
}
fn discover(path: &Path, files: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_file() {
        if path.extension().is_some_and(|e| e == "zen") {
            files.insert(fs::canonicalize(path)?);
        }
    } else if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || [
                    "target",
                    "build",
                    "dist",
                    "node_modules",
                    "vendor",
                    "generated",
                    "__generated__",
                ]
                .contains(&name.as_ref())
            {
                continue;
            }
            discover(&entry.path(), files)?;
        }
    }
    Ok(())
}
fn atomic_write(path: &Path, original: &str, text: &str) -> io::Result<()> {
    // Same-directory replacement is atomic; create_new never overwrites another
    // process's temp file. Preserve permissions and refuse a concurrently edited file.
    let mut attempt = 0;
    let (temp, mut file) = loop {
        let temp = path.with_file_name(format!(".zen-fmt-{}-{attempt}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => attempt += 1,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.set_permissions(fs::metadata(path)?.permissions())?;
        file.sync_all()?;
        if fs::read_to_string(path)? != original {
            return Err(io::Error::other("file changed while formatting"));
        }
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
