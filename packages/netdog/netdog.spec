%global _cross_first_party 1
%undefine _debugsource_packages

Name: %{_cross_os}netdog
Version: 0.1.1
Release: 0%{?dist}
Summary: Bottlerocket network configuration helper
License: Apache-2.0 OR MIT
URL: https://github.com/bottlerocket-os/bottlerocket

Source2: run-netdog.mount
Source3: write-network-status.service
Source4: netdog-tmpfiles.conf
Source5: disable-udp-offload.service
Source6: 00-resolved.conf
Source7: generate-network-config.service

BuildRequires: %{_cross_os}glibc-devel
Requires: %{_cross_os}netdog
%description
%{summary}.

%package -n %{_cross_os}netdog-systemd-networkd
Summary: Bottlerocket network configuration helper
Requires: %{_cross_os}systemd-networkd
Requires: %{_cross_os}systemd-resolved
%description -n %{_cross_os}netdog-systemd-networkd
%{summary}.

%package -n %{_cross_os}netdog-wicked
Summary: Bottlerocket network configuration helper
Requires: %{_cross_os}wicked
%description -n %{_cross_os}netdog-wicked
%{summary}.

%prep
%setup -T -c
%cargo_prep

%build
mkdir bin

echo "** Build Netdog for SystemD NetworkD"
%cargo_build --manifest-path %{_builddir}/sources/Cargo.toml \
    -p netdog \
    --features systemd-networkd \
    --target-dir ${HOME}/.cache/networkd
echo "** Build Netdog for Wicked"
%cargo_build --manifest-path %{_builddir}/sources/Cargo.toml \
    -p netdog \
    --features wicked \
    --target-dir ${HOME}/.cache/wicked
    
%install
install -d %{buildroot}%{_cross_bindir}
install -d %{buildroot}%{_cross_tmpfilesdir}
install -d %{buildroot}%{_cross_unitdir}
install -d %{buildroot}%{_cross_libdir}
install -p -m 0644 %{S:4} %{buildroot}%{_cross_tmpfilesdir}/netdog.conf
install -p -m 0644 %{S:2} %{S:3} %{S:7} %{buildroot}%{_cross_unitdir}
install -d %{buildroot}%{_cross_libdir}/systemd/resolved.conf.d
install -p -m 0644 %{S:6} %{buildroot}%{_cross_libdir}/systemd/resolved.conf.d
%if %{with vmware_platform}
install -p -m 0644 %{S:5} %{buildroot}%{_cross_unitdir}
%endif
install -p -m 0755 ${HOME}/.cache/networkd/%{__cargo_target}/release/netdog %{buildroot}%{_cross_bindir}/netdog-systemd-networkd
install -p -m 0755 ${HOME}/.cache/wicked/%{__cargo_target}/release/netdog %{buildroot}%{_cross_bindir}/netdog-wicked

%files -n %{_cross_os}netdog-systemd-networkd
%{_cross_bindir}/netdog-systemd-networkd
%{_cross_tmpfilesdir}/netdog.conf
%{_cross_unitdir}/generate-network-config.service
%{_cross_unitdir}/run-netdog.mount
%if %{with vmware_platform}
%{_cross_unitdir}/disable-udp-offload.service
%endif
%{_cross_unitdir}/write-network-status.service
%dir %{_cross_libdir}/systemd/resolved.conf.d
%{_cross_libdir}/systemd/resolved.conf.d/00-resolved.conf

%files -n %{_cross_os}netdog-wicked
%{_cross_bindir}/netdog-wicked
%{_cross_tmpfilesdir}/netdog.conf
%{_cross_unitdir}/generate-network-config.service
%{_cross_unitdir}/run-netdog.mount
%if %{with vmware_platform}
%{_cross_unitdir}/disable-udp-offload.service
%endif

%post -n %{_cross_os}netdog-wicked
/bin/ln -s %{_cross_bindir}/netdog-wicked %{_cross_bindir}/netdog

%post -n %{_cross_os}netdog-systemd-networkd
/bin/ln -s %{_cross_bindir}/netdog-systemd-networkd %{_cross_bindir}/netdog