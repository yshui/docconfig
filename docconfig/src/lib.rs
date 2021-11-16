pub use docconfig_derive::DocConfig;

pub struct WritePrefixed<W>(W, String);

impl<W> WritePrefixed<W> {
    pub fn new(w: W, s: String) -> Self {
        Self(w, s)
    }
}

/// Prepend a prefix to the things then write them to the inner writter
impl<W: std::io::Write> std::io::Write for WritePrefixed<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(self.0.write(self.1.as_bytes())? + self.0.write(buf)?)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}


pub trait DocConfig {
    type Error;
    /// w: writer
    /// def: default value for this type
    /// path: path to this type from root
    fn write_doc_config(
        w: &mut impl std::io::Write,
        path: &[&str],
        def: Option<&Self>,
    ) -> Result<(), Self::Error>;

    /// whether this type has internal structure. if true
    /// the outer type will start a section for this type
    fn is_plain() -> bool;
}

impl DocConfig for String {
    type Error = std::io::Error;
    fn write_doc_config(w: &mut impl std::io::Write, path: &[&str], def: Option<&Self>) -> Result<(), Self::Error> {
        writeln!(w, "## a string")?;
        if let Some(s) = def {
            writeln!(w, "{} = \"{}\"", path.last().unwrap(), s)?;
        } else {
            writeln!(w, "# {} = ...", path.last().unwrap())?;
        }
        Ok(())
    }
    fn is_plain() -> bool {
        true
    }
}

impl DocConfig for f32 {
    type Error = std::io::Error;
    fn write_doc_config(w: &mut impl std::io::Write, path: &[&str], def: Option<&Self>) -> Result<(), Self::Error> {
        writeln!(w, "## a real number")?;
        if let Some(s) = def {
            writeln!(w, "{} = {}", path.last().unwrap(), s)?;
        } else {
            writeln!(w, "# {} = ...", path.last().unwrap())?;
        }
        Ok(())
    }
    fn is_plain() -> bool {
        true
    }
}
