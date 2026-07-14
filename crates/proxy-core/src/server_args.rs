use std::{
    ffi::OsString,
    fmt,
    net::{IpAddr, Ipv4Addr},
};

use crate::client_auth::{validate_api_key, ClientApiKey};

pub const SERVER_USAGE: &str = concat!(
    "Usage: proxy-server [--host <ip>] [--api-key <value>]...\n",
    "\n",
    "Options:\n",
    "  --host <ip>        Bind IP address (default: 127.0.0.1)\n",
    "  --api-key <value>  Protect OpenAI /v1/* routes; may be repeated\n",
    "  -h, --help         Print help\n",
);

pub struct ServerArgs {
    pub host: IpAddr,
    pub api_keys: Vec<ClientApiKey>,
    pub show_help: bool,
}

impl fmt::Debug for ServerArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerArgs")
            .field("host", &self.host)
            .field("api_key_count", &self.api_keys.len())
            .field("show_help", &self.show_help)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServerArgsError {
    MissingApiKeyValue,
    InvalidApiKey,
    MissingHostValue,
    InvalidHost,
    UnknownArgument,
}

impl fmt::Display for ServerArgsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingApiKeyValue => "--api-key requires a value",
            Self::InvalidApiKey => "--api-key must use the required sk- format",
            Self::MissingHostValue => "--host requires an IP address",
            Self::InvalidHost => "--host must be a valid IP address",
            Self::UnknownArgument => "unknown standalone server argument",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ServerArgsError {}

pub fn parse_server_args<I, S>(args: I) -> Result<ServerArgs, ServerArgsError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into).peekable();
    let mut host = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut api_keys = Vec::new();
    let mut show_help = false;

    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--api-key") => {
                let value = take_value(&mut args, ServerArgsError::MissingApiKeyValue)?;
                let value = value.to_str().ok_or(ServerArgsError::InvalidApiKey)?;
                let key = validate_api_key(value).map_err(|_| ServerArgsError::InvalidApiKey)?;
                if !api_keys.contains(&key) {
                    api_keys.push(key);
                }
            }
            Some("--host") => {
                let value = take_value(&mut args, ServerArgsError::MissingHostValue)?;
                let value = value.to_str().ok_or(ServerArgsError::InvalidHost)?;
                host = value.parse().map_err(|_| ServerArgsError::InvalidHost)?;
            }
            Some("-h" | "--help") => show_help = true,
            _ => return Err(ServerArgsError::UnknownArgument),
        }
    }

    Ok(ServerArgs {
        host,
        api_keys,
        show_help,
    })
}

fn take_value<I>(
    args: &mut std::iter::Peekable<I>,
    missing_error: ServerArgsError,
) -> Result<OsString, ServerArgsError>
where
    I: Iterator<Item = OsString>,
{
    let value = args.next().ok_or(missing_error)?;
    if value.to_str().is_some_and(|value| value.starts_with('-')) {
        return Err(missing_error);
    }
    Ok(value)
}
