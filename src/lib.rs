use std::{fmt, fs, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Doctor,
    Validate(PathBuf),
    Inspect(PathBuf),
}

impl Command {
    pub fn parse(args: &[String]) -> Result<Self, RiffError> {
        match args {
            [] => Ok(Self::Help),
            [flag] if flag == "-h" || flag == "--help" || flag == "help" => Ok(Self::Help),
            [flag] if flag == "-V" || flag == "--version" || flag == "version" => Ok(Self::Version),
            [cmd] if cmd == "doctor" => Ok(Self::Doctor),
            [cmd, path] if cmd == "validate" => Ok(Self::Validate(path.into())),
            [cmd, path] if cmd == "inspect" => Ok(Self::Inspect(path.into())),
            _ => Err(RiffError::Usage(
                "unknown command or invalid arguments".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<String>,
}

impl Playlist {
    pub fn parse(source: &str) -> Result<Self, RiffError> {
        let mut lines = source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'));
        let header = lines
            .next()
            .ok_or_else(|| RiffError::Parse("missing playlist declaration".into()))?;

        if !header.starts_with("playlist ") || !header.ends_with('{') {
            return Err(RiffError::Parse("expected `playlist \"name\" {`".into()));
        }

        let name = header
            .trim_start_matches("playlist ")
            .trim_end_matches('{')
            .trim()
            .trim_matches('"')
            .to_string();

        if name.is_empty() {
            return Err(RiffError::Parse("playlist name cannot be empty".into()));
        }

        let mut tracks = Vec::new();
        let mut closed = false;

        for line in lines {
            if line == "}" {
                closed = true;
                break;
            }

            if let Some(value) = line.strip_prefix("track ") {
                let value = value.trim().trim_matches('"');
                if value.is_empty() {
                    return Err(RiffError::Parse("track cannot be empty".into()));
                }
                tracks.push(value.to_string());
                continue;
            }

            return Err(RiffError::Parse(format!("unsupported statement: `{line}`")));
        }

        if !closed {
            return Err(RiffError::Parse("playlist block is not closed".into()));
        }

        Ok(Self { name, tracks })
    }
}

#[derive(Debug)]
pub enum RiffError {
    Io(std::io::Error),
    Parse(String),
    Usage(String),
}

impl fmt::Display for RiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::Usage(msg) => write!(f, "{msg}\n\n{}", help()),
        }
    }
}

impl From<std::io::Error> for RiffError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn run(command: Command) -> Result<String, RiffError> {
    match command {
        Command::Help => Ok(help()),
        Command::Version => Ok(format!("riff {}", env!("CARGO_PKG_VERSION"))),
        Command::Doctor => Ok(doctor()),
        Command::Validate(path) => {
            let source = fs::read_to_string(&path)?;
            let playlist = Playlist::parse(&source)?;
            Ok(format!(
                "✓ {} is valid ({} tracks)",
                playlist.name,
                playlist.tracks.len()
            ))
        }
        Command::Inspect(path) => {
            let source = fs::read_to_string(&path)?;
            let playlist = Playlist::parse(&source)?;
            let body = playlist
                .tracks
                .iter()
                .enumerate()
                .map(|(index, track)| format!("{:>3}. {track}", index + 1))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(format!(
                "{}\n{} tracks\n\n{}",
                playlist.name,
                playlist.tracks.len(),
                body
            ))
        }
    }
}

pub fn help() -> String {
    format!(
        "riff {}\nMusic as code, from the terminal.\n\nUSAGE:\n  riff <COMMAND>\n\nCOMMANDS:\n  doctor           Check the local Riff environment\n  validate <file>  Validate a .riff playlist\n  inspect <file>   Parse and print a .riff playlist\n  help             Print this help\n  version          Print version",
        env!("CARGO_PKG_VERSION")
    )
}

fn doctor() -> String {
    format!(
        "Riff doctor\n  version      {}\n  rust target  {}\n  playlist DSL ready\n  spotify      not configured yet\n  playback     not implemented yet",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_playlist() {
        let source = r#"
            playlist "coding-metal" {
                track "Black Sabbath - War Pigs"
                track "Dio - Holy Diver"
            }
        "#;

        let playlist = Playlist::parse(source).expect("playlist should parse");
        assert_eq!(playlist.name, "coding-metal");
        assert_eq!(playlist.tracks.len(), 2);
    }

    #[test]
    fn rejects_unknown_statement() {
        let source = "playlist \"x\" {\nshuffle true\n}";
        assert!(Playlist::parse(source).is_err());
    }
}
