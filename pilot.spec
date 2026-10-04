Name:           pilot
Version:        0.1.0
Release:        1%{?dist}
Summary:        Chromecast Voice Remote PC controller and GTK4 settings suite

License:        MIT
URL:            https://github.com/irelandqlan/pilot
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  systemd-rpm-macros

Requires:       python3
Requires:       python3-evdev
Requires:       gtk4
Requires:       libadwaita
Requires:       pipewire
Requires:       bluez

# Disable debuginfo package and terminate on missing build-ids
%global debug_package %{nil}
%undefine _missing_build_ids_terminate_build

%description
Pilot is a native Linux application and daemon suite that enables using a
Google Chromecast with Google TV voice remote as a versatile PC controller.
It provides smooth mouse navigation, customizable multi-layer button actions,
and voice typing dictation integrated with GNOME Background Apps.

%prep
%autosetup

%build
cargo build --release

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_libexecdir}/pilot
mkdir -p %{buildroot}%{_userunitdir}
mkdir -p %{buildroot}%{_udevrulesdir}
mkdir -p %{buildroot}%{_datadir}/applications
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/actions
mkdir -p %{buildroot}%{_metainfodir}
mkdir -p %{buildroot}%{_datadir}/pilot

# GUI binary
install -m 755 target/release/pilot %{buildroot}%{_bindir}/pilot
ln -s pilot %{buildroot}%{_bindir}/chromecast-settings

# CLI utility
install -m 755 bin/pilot-ctl %{buildroot}%{_bindir}/pilot-ctl
ln -s pilot-ctl %{buildroot}%{_bindir}/chromecast-ctl

# Backend Daemons & Helpers
install -m 755 voice_daemon.py %{buildroot}%{_libexecdir}/pilot/voice_daemon.py
install -m 755 bin/atvvoice %{buildroot}%{_libexecdir}/pilot/atvvoice

# Default config template
install -m 644 config.toml %{buildroot}%{_datadir}/pilot/config.toml

# Systemd user service
install -m 644 data/pilot.service %{buildroot}%{_userunitdir}/pilot.service

# Udev rules for /dev/uinput
install -m 644 data/70-pilot-uinput.rules %{buildroot}%{_udevrulesdir}/70-pilot-uinput.rules

# Desktop launcher, icon, symbolic actions, and AppStream metainfo
install -m 644 data/io.github.magnotec.Pilot.desktop %{buildroot}%{_datadir}/applications/io.github.magnotec.Pilot.desktop
install -m 644 data/io.github.magnotec.Pilot.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.magnotec.Pilot.svg
install -m 644 data/icons/*.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/actions/
install -m 644 data/io.github.magnotec.Pilot.metainfo.xml %{buildroot}%{_metainfodir}/io.github.magnotec.Pilot.metainfo.xml

%post
udevadm control --reload-rules 2>/dev/null || :
udevadm trigger --subsystem-match=misc 2>/dev/null || :
%systemd_user_post pilot.service

%preun
%systemd_user_preun pilot.service

%postun
%systemd_user_postun_with_restart pilot.service

%files
%{_bindir}/pilot
%{_bindir}/chromecast-settings
%{_bindir}/pilot-ctl
%{_bindir}/chromecast-ctl
%{_libexecdir}/pilot/
%{_userunitdir}/pilot.service
%{_udevrulesdir}/70-pilot-uinput.rules
%{_datadir}/applications/io.github.magnotec.Pilot.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.magnotec.Pilot.svg
%{_datadir}/icons/hicolor/scalable/actions/*.svg
%{_metainfodir}/io.github.magnotec.Pilot.metainfo.xml
%{_datadir}/pilot/config.toml
