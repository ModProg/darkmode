#![warn(clippy::pedantic, missing_docs, clippy::cargo)]
#![allow(clippy::wildcard_imports)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//! This crate currently only supports Linux. Though I'm not opposed to add
//! other platforms. It uses the
//! [XDG Desktop Portal Settings](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html#org-freedesktop-portal-settings-settingchanged).
//!
//! It is intended as a minimal crate to be used on top of `winit`'s built-in
//! dark mode detection on other OSes.

use std::fmt::Display;
use std::thread;
use std::time::Duration;

use dbus::arg::{ReadAll, RefArg, Variant};
use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
use dbus::blocking::{Connection, Proxy};
use dbus::message::SignalArgs;
use dbus::Message;

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, Hash)]
#[repr(u32)]
pub enum Mode {
    #[default]
    Default,
    Dark,
    Light,
}

fn mode_from_u32(mode: u32) -> Mode {
    match mode {
        1 => Mode::Dark,
        2 => Mode::Light,
        _ => Mode::Default,
    }
}

const INTERFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";

#[derive(Debug)]
pub struct Error(Box<dyn std::error::Error>);

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl Error {
    pub fn new(error: impl std::error::Error + 'static) -> Self {
        Self(Box::new(error))
    }
}

impl From<dbus::Error> for Error {
    fn from(value: dbus::Error) -> Self {
        Self::new(value)
    }
}

fn proxy() -> Result<Proxy<'static, Box<Connection>>, Error> {
    let connection = Connection::new_session()?;
    Ok(Proxy::new(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        Duration::from_millis(100),
        Box::new(connection),
    ))
}

pub fn detect() -> Result<Mode, Error> {
    let proxy = proxy()?;
    let color_scheme = proxy.method_call::<(Variant<u32>,), _, _, _>(
        INTERFACE,
        "ReadOne",
        (NAMESPACE, COLOR_SCHEME),
    );

    match color_scheme {
        Ok((Variant(color_scheme),)) => Ok(mode_from_u32(color_scheme)),
        _ if proxy.get::<u32>("org.freedesktop.portal.Settings", "version")? < 2 => {
            Ok(mode_from_u32(
                proxy
                    .method_call::<(Variant<Variant<u32>>,), _, _, _>(
                        INTERFACE,
                        "Read",
                        (NAMESPACE, COLOR_SCHEME),
                    )?
                    .0
                    .0
                    .0,
            ))
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug)]
struct SettingChanged {
    namespace: String,
    key: String,
    value: Variant<Box<dyn RefArg>>,
}

impl SignalArgs for SettingChanged {
    const INTERFACE: &'static str = INTERFACE;
    const NAME: &'static str = "SettingChanged";
}

impl ReadAll for SettingChanged {
    fn read(i: &mut dbus::arg::Iter) -> Result<Self, dbus::arg::TypeMismatchError> {
        Ok(Self {
            namespace: i.read()?,
            key: i.read()?,
            value: i.read()?,
        })
    }
}

pub fn subscribe(mut call_back: impl FnMut(Mode) + Send + 'static) -> Result<(), Error> {
    call_back(detect()?);
    let proxy = proxy()?;

    let token = proxy.match_signal(
        move |ref dbg @ SettingChanged {
                  ref namespace,
                  ref key,
                  ref value,
              },
              _: &Connection,
              _: &Message| {
            if namespace == NAMESPACE && key == COLOR_SCHEME {
                if let Some(value) = value.0.as_u64() {
                    call_back(mode_from_u32(value.try_into().unwrap_or_default()));
                }
            }
            true
        },
    )?;
    thread::spawn(move || {
        let _ = token;

        loop {
            _ = proxy.connection.process(Duration::from_secs(1));
        }
    });
    Ok(())
}
