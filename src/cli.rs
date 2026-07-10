#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    Host,
    Client,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Host => write!(f, "host"),
            Mode::Client => write!(f, "client"),
        }
    }
}

#[derive(clap::Parser, Debug)]
#[command(name = "peerlink", about = "Remote desktop sharing")]
pub struct Cli {
    /// Start in host or client mode
    pub mode: Option<Mode>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_default_is_none() {
        let cli = Cli { mode: None };
        assert_eq!(cli.mode, None);
    }

    #[test]
    fn mode_display() {
        assert_eq!(Mode::Host.to_string(), "host");
        assert_eq!(Mode::Client.to_string(), "client");
    }

    #[test]
    fn mode_equality() {
        assert_eq!(Mode::Host, Mode::Host);
        assert_ne!(Mode::Host, Mode::Client);
    }

    #[test]
    fn cli_parse_host() {
        let cli = Cli::parse_from(["peerlink", "host"]);
        assert_eq!(cli.mode, Some(Mode::Host));
    }

    #[test]
    fn cli_parse_client() {
        let cli = Cli::parse_from(["peerlink", "client"]);
        assert_eq!(cli.mode, Some(Mode::Client));
    }

    #[test]
    fn cli_parse_no_args() {
        let cli = Cli::parse_from(["peerlink"]);
        assert_eq!(cli.mode, None);
    }
}
