use std::{fmt, fs, io, io::Write, path::PathBuf};

pub mod player;

const DEFAULT_PLAYLIST: &str = r#"playlist "my-playlist" {
    track "Black Sabbath - War Pigs"
    track "Dio - Holy Diver"
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Doctor,
    Init(PathBuf),
    Validate(PathBuf),
    Inspect(PathBuf),
    Play(PathBuf),
    Player(Option<String>),
    Search { query: String, limit: usize },
    InspectTrack(String),
    Pick(String),
}

impl Command {
    pub fn parse(args: &[String]) -> Result<Self, RiffError> {
        match args {
            [] => Ok(Self::Help),
            [flag] if flag == "-h" || flag == "--help" || flag == "help" => Ok(Self::Help),
            [flag] if flag == "-V" || flag == "--version" || flag == "version" => Ok(Self::Version),
            [cmd] if cmd == "doctor" => Ok(Self::Doctor),
            [cmd] if cmd == "init" => Ok(Self::Init("playlist.riff".into())),
            [cmd, path] if cmd == "init" => Ok(Self::Init(path.into())),
            [cmd, path] if cmd == "validate" => Ok(Self::Validate(path.into())),
            [cmd, path] if cmd == "inspect" => Ok(Self::Inspect(path.into())),
            [cmd, path] if cmd == "play" => Ok(Self::Play(path.into())),
            [cmd] if cmd == "player" => Ok(Self::Player(None)),
            [cmd, uri] if cmd == "player" => Ok(Self::Player(Some(uri.clone()))),
            [cmd, query] if cmd == "search" => Ok(Self::Search {
                query: query.clone(),
                limit: 10,
            }),
            [cmd, query, flag, limit] if cmd == "search" && flag == "--limit" => {
                let limit = limit.parse::<usize>().map_err(|_| {
                    RiffError::Usage("search --limit must be a positive integer".into())
                })?;
                if limit == 0 {
                    return Err(RiffError::Usage(
                        "search --limit must be greater than zero".into(),
                    ));
                }
                Ok(Self::Search {
                    query: query.clone(),
                    limit,
                })
            }
            [cmd, uri] if cmd == "inspect-track" => Ok(Self::InspectTrack(uri.clone())),
            [cmd, query] if cmd == "pick" => Ok(Self::Pick(query.clone())),
            _ => Err(RiffError::Usage(
                "unknown command or invalid arguments".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub label: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
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

        let name = parse_playlist_header(header)?;
        let mut tracks = Vec::new();
        let mut closed = false;

        for line in lines.by_ref() {
            if line == "}" {
                closed = true;
                break;
            }

            let value = line
                .strip_prefix("track ")
                .ok_or_else(|| RiffError::Parse(format!("unsupported statement: `{line}`")))?;

            tracks.push(parse_track(value)?);
        }

        if !closed {
            return Err(RiffError::Parse("playlist block is not closed".into()));
        }

        if lines.next().is_some() {
            return Err(RiffError::Parse(
                "unexpected content after playlist block".into(),
            ));
        }

        Ok(Self {
            name: name.to_string(),
            tracks,
        })
    }
}

fn parse_track(value: &str) -> Result<Track, RiffError> {
    let value = value.trim();
    if !value.starts_with('"') {
        return Err(RiffError::Parse("track must start with a quoted label".into()));
    }

    let closing_quote = value[1..]
        .find('"')
        .map(|index| index + 1)
        .ok_or_else(|| RiffError::Parse("track label is missing its closing quote".into()))?;
    let label = &value[1..closing_quote];
    if label.is_empty() {
        return Err(RiffError::Parse("track cannot be empty".into()));
    }

    let rest = value[closing_quote + 1..].trim();
    let id = if rest.is_empty() {
        None
    } else {
        let raw_id = rest
            .strip_prefix("id=")
            .ok_or_else(|| RiffError::Parse(format!("unsupported track selector: `{rest}`")))?;
        let id = parse_quoted(raw_id, "track id")?;
        if id.is_empty() {
            return Err(RiffError::Parse("track id cannot be empty".into()));
        }
        Some(id.to_string())
    };

    Ok(Track {
        label: label.to_string(),
        id,
    })
}

fn parse_playlist_header(header: &str) -> Result<&str, RiffError> {
    let value = header
        .strip_prefix("playlist ")
        .and_then(|value| value.strip_suffix('{'))
        .map(str::trim)
        .ok_or_else(|| RiffError::Parse("expected `playlist \"name\" {`".into()))?;

    let name = parse_quoted(value, "playlist name")?;
    if name.is_empty() {
        return Err(RiffError::Parse("playlist name cannot be empty".into()));
    }

    Ok(name)
}

fn parse_quoted<'a>(value: &'a str, label: &str) -> Result<&'a str, RiffError> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(RiffError::Parse(format!("{label} must be quoted")));
    }

    Ok(&value[1..value.len() - 1])
}

#[derive(Debug)]
pub enum RiffError {
    Io(std::io::Error),
    Parse(String),
    Usage(String),
    AlreadyExists(PathBuf),
    Player(String),
}

impl fmt::Display for RiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::Usage(msg) => write!(f, "{msg}\n\n{}", help()),
            Self::AlreadyExists(path) => write!(
                f,
                "{} already exists; choose another path or remove it first",
                path.display()
            ),
            Self::Player(msg) => write!(f, "player error: {msg}"),
        }
    }
}

impl From<std::io::Error> for RiffError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub async fn run(command: Command) -> Result<String, RiffError> {
    match command {
        Command::Help => Ok(help()),
        Command::Version => Ok(format!("riff {}", env!("CARGO_PKG_VERSION"))),
        Command::Doctor => Ok(doctor()),
        Command::Init(path) => init(path),
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
                .map(|(index, track)| match track.id.as_deref() {
                    Some(id) => format!("{:>3}. {}\n     id: {id}", index + 1, track.label),
                    None => format!("{:>3}. {}", index + 1, track.label),
                })
                .collect::<Vec<_>>()
                .join("\n");

            if body.is_empty() {
                Ok(format!("{}\n0 tracks", playlist.name))
            } else {
                Ok(format!(
                    "{}\n{} tracks\n\n{}",
                    playlist.name,
                    playlist.tracks.len(),
                    body
                ))
            }
        }
        Command::Play(path) => {
            let source = fs::read_to_string(&path)?;
            let playlist = Playlist::parse(&source)?;
            if playlist.tracks.is_empty() {
                return Err(RiffError::Parse("playlist has no tracks to play".into()));
            }

            println!(
                "Loading `{}` from {} ({} tracks)...",
                playlist.name,
                path.display(),
                playlist.tracks.len()
            );

            let options = player::PlayerOptions {
                tracks: playlist
                    .tracks
                    .into_iter()
                    .map(|track| player::TrackRequest {
                        label: track.label,
                        id: track.id,
                    })
                    .collect(),
                ..player::PlayerOptions::default()
            };
            player::run(options).await.map_err(RiffError::Player)?;
            Ok(String::new())
        }
        Command::Player(context_uri) => {
            let options = player::PlayerOptions {
                context_uri,
                ..player::PlayerOptions::default()
            };
            player::run(options).await.map_err(RiffError::Player)?;
            Ok(String::new())
        }
        Command::Search { query, limit } => {
            let candidates = player::search(&query, limit)
                .await
                .map_err(RiffError::Player)?;
            if candidates.is_empty() {
                return Ok(format!("No Spotify tracks found for `{query}`."));
            }
            Ok(format_candidates(&query, &candidates))
        }
        Command::InspectTrack(uri) => {
            let candidate = player::inspect_track(&uri)
                .await
                .map_err(RiffError::Player)?;
            Ok(format_candidate_details(&candidate))
        }
        Command::Pick(query) => {
            let candidates = player::search(&query, 10)
                .await
                .map_err(RiffError::Player)?;
            if candidates.is_empty() {
                return Ok(format!("No Spotify tracks found for `{query}`."));
            }

            println!("{}", format_candidates(&query, &candidates));
            print!("\nChoose a recording [1-{}]: ", candidates.len());
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let selection = input
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|value| (1..=candidates.len()).contains(value))
                .ok_or_else(|| RiffError::Usage("invalid recording selection".into()))?;
            let candidate = &candidates[selection - 1];
            Ok(format!(
                "track \"{}\" id=\"{}\"",
                escape_riff_string(&query),
                candidate.uri
            ))
        }
    }
}

fn format_candidates(query: &str, candidates: &[player::SearchCandidate]) -> String {
    let mut lines = vec![format!("Spotify recordings for `{query}`:")];
    for (index, candidate) in candidates.iter().enumerate() {
        lines.push(format!(
            "\n{}. {}\n   id: {}",
            index + 1,
            candidate.display_name(),
            candidate.uri
        ));
        for (key, value) in candidate.metadata.iter().take(6) {
            lines.push(format!("   {key}: {value}"));
        }
    }
    lines.join("\n")
}

fn format_candidate_details(candidate: &player::SearchCandidate) -> String {
    let mut lines = vec![format!("{}\nid: {}", candidate.display_name(), candidate.uri)];
    if candidate.metadata.is_empty() {
        lines.push("metadata: not returned by Spotify context resolver".to_string());
    } else {
        lines.push("metadata:".to_string());
        for (key, value) in &candidate.metadata {
            lines.push(format!("  {key}: {value}"));
        }
    }
    lines.join("\n")
}

fn escape_riff_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn init(path: PathBuf) -> Result<String, RiffError> {
    if path.exists() {
        return Err(RiffError::AlreadyExists(path));
    }

    fs::write(&path, DEFAULT_PLAYLIST)?;
    Ok(format!(
        "✓ created {}\n\nNext:\n  riff validate {}\n  riff inspect {}\n  riff play {}",
        path.display(),
        path.display(),
        path.display(),
        path.display()
    ))
}

pub fn help() -> String {
    format!(
        "riff {}\nMusic as code, from the terminal.\n\nUSAGE:\n  riff <COMMAND>\n\nCOMMANDS:\n  init [file]         Create a starter playlist (default: playlist.riff)\n  doctor              Check the local Riff environment\n  validate <file>     Validate a .riff playlist\n  inspect <file>      Parse and print a .riff playlist\n  play <file>         Resolve a .riff playlist on Spotify and start playback\n  search <query>      Explore Spotify recordings and stable track IDs\n  inspect-track <id>  Inspect one Spotify track ID\n  pick <query>        Interactively choose a recording and print pinned DSL\n  player [uri]        Start Riff as a local Spotify Connect player\n  help                Print this help\n  version             Print version",
        env!("CARGO_PKG_VERSION")
    )
}

fn doctor() -> String {
    format!(
        "Riff doctor\n  version      {}\n  platform     {}-{}\n  playlist DSL ready\n  spotify      player + discovery + pinned track ids available (Premium required)\n  playback     librespot / local audio backend",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
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
        assert_eq!(playlist.tracks[0].label, "Black Sabbath - War Pigs");
        assert_eq!(playlist.tracks[0].id, None);
    }

    #[test]
    fn parses_pinned_track_id() {
        let source = r#"
            playlist "coding-metal" {
                track "Black Sabbath - War Pigs" id="spotify:track:abc123"
            }
        "#;
        let playlist = Playlist::parse(source).expect("playlist should parse");
        assert_eq!(
            playlist.tracks[0],
            Track {
                label: "Black Sabbath - War Pigs".to_string(),
                id: Some("spotify:track:abc123".to_string())
            }
        );
    }

    #[test]
    fn default_playlist_template_is_valid() {
        let playlist = Playlist::parse(DEFAULT_PLAYLIST).expect("default template should parse");
        assert_eq!(playlist.name, "my-playlist");
        assert_eq!(playlist.tracks.len(), 2);
    }

    #[test]
    fn rejects_unknown_statement() {
        let source = "playlist \"x\" {\nshuffle true\n}";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn rejects_unquoted_playlist_name() {
        let source = "playlist coding {\n}";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn rejects_unquoted_track() {
        let source = "playlist \"coding\" {\ntrack Black Sabbath - War Pigs\n}";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn rejects_content_after_playlist() {
        let source = "playlist \"coding\" {\n}\ntrack \"orphan\"";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn init_defaults_to_playlist_riff() {
        let args = vec!["init".to_string()];
        assert_eq!(
            Command::parse(&args).expect("command should parse"),
            Command::Init(PathBuf::from("playlist.riff"))
        );
    }

    #[test]
    fn parses_play_command() {
        let args = vec!["play".to_string(), "coding-metal.riff".to_string()];
        assert_eq!(
            Command::parse(&args).expect("command should parse"),
            Command::Play(PathBuf::from("coding-metal.riff"))
        );
    }

    #[test]
    fn parses_search_limit() {
        let args = vec![
            "search".to_string(),
            "War Pigs".to_string(),
            "--limit".to_string(),
            "20".to_string(),
        ];
        assert_eq!(
            Command::parse(&args).expect("command should parse"),
            Command::Search {
                query: "War Pigs".to_string(),
                limit: 20
            }
        );
    }

    #[test]
    fn parses_player_with_context_uri() {
        let args = vec![
            "player".to_string(),
            "spotify:album:1ATL5GLyefJaxhQzSPVrLX".to_string(),
        ];
        assert_eq!(
            Command::parse(&args).expect("command should parse"),
            Command::Player(Some("spotify:album:1ATL5GLyefJaxhQzSPVrLX".to_string()))
        );
    }
}
