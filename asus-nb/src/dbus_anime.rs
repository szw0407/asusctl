// This code was autogenerated with `dbus-codegen-rust -s -d org.asuslinux.Daemon -p /org/asuslinux/Anime -m None -f org.asuslinux.Daemon -c blocking`, see https://github.com/diwic/dbus-rs
use dbus as dbus;
#[allow(unused_imports)]
use dbus::arg;
use dbus::blocking;

pub trait OrgAsuslinuxDaemon {
    fn set_anime(&self, input: Vec<Vec<u8>>) -> Result<(), dbus::Error>;
}

impl<'a, T: blocking::BlockingSender, C: ::std::ops::Deref<Target=T>> OrgAsuslinuxDaemon for blocking::Proxy<'a, C> {

    fn set_anime(&self, input: Vec<Vec<u8>>) -> Result<(), dbus::Error> {
        self.method_call("org.asuslinux.Daemon", "SetAnime", (input, ))
    }
}