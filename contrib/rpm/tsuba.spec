# RPM spec for Fedora and derivatives. Build from a checkout:
#   tar --transform 's|^|tsuba-0.1.0/|' --exclude=target -czf ~/rpmbuild/SOURCES/tsuba-0.1.0.tar.gz .
#   rpmbuild -ba contrib/rpm/tsuba.spec
# Not yet tried on a Fedora system: check the package names against your release before publishing.
Name:           tsuba
Version:        0.1.0
Release:        1%{?dist}
Summary:        Open-source configuration for Razer keyboards and mice
License:        GPL-2.0-or-later
URL:            https://github.com/JenAriDev/tsuba
Source0:        %{name}-%{version}.tar.gz
ExclusiveArch:  x86_64

BuildRequires:  cargo, rust, clang, pkgconf-pkg-config, gettext, systemd-rpm-macros
BuildRequires:  qt6-qtbase-devel, qt6-qtdeclarative-devel, qt6-qtsvg-devel, kf6-ki18n-devel
BuildRequires:  pipewire-devel, pulseaudio-libs-devel, mingw64-gcc, mingw32-gcc
Requires:       qt6-qtdeclarative, qt6-qtsvg, kf6-ki18n, qqc2-breeze-style
Requires:       pipewire-libs, pulseaudio-libs, pulseaudio-utils, xdg-desktop-portal, xdg-utils
Requires:       google-roboto-fonts, systemd-udev
Recommends:     kf6-kglobalaccel
Suggests:       wine

%description
Remap keys and buttons, record macros, set DPI and lighting, and use Lighting Studio, without
Razer's Windows software. Includes the user service tsubad, the Tsuba app and the tsuba
command-line tool. Not affiliated with Razer Inc.

%prep
%autosetup

%build
export RUSTFLAGS="%{?build_rustflags} --cfg tsuba_packaged"
cargo build --release --workspace
make -C shim
sh po/build.sh

%install
contrib/install-tree.sh %{buildroot}

%post
udevadm control --reload-rules >/dev/null 2>&1 || :
udevadm trigger --subsystem-match=hidraw --attr-match=idVendor=1532 >/dev/null 2>&1 || :
echo "Start the Tsuba service for your user: systemctl --user enable --now tsubad"

%postun
udevadm control --reload-rules >/dev/null 2>&1 || :

%files
%license LICENSE
%doc %{_docdir}/tsuba
%{_bindir}/tsuba
%{_bindir}/tsubad
%{_bindir}/tsuba-app
%{_bindir}/tsuba-handover
%{_bindir}/tsuba-handover-restore
%{_prefix}/lib/systemd/user/tsubad.service
%{_prefix}/lib/udev/rules.d/70-tsuba.rules
%{_datadir}/tsuba
%{_datadir}/applications/io.github.jenaridev.tsuba.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.jenaridev.tsuba.svg
%{_datadir}/locale/*/LC_MESSAGES/tsuba.mo
