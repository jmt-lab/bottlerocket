%global _cross_first_party 1
%undefine _debugsoure_packages

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

Requires: %{_cross_os}netdog

%package -n %{_cross_os}netdog-wicked
Summary: Bottlerocket network configuration helper
Requires: %{_cross_os}wicked
%description -n %{_cross_os}netdog-wicked
%{summary}.

%package -n %{_cross_os}netdog-systemd-networkd
Summary: Bottlerocket network configuration helper
Requires: %{_cross_os}systemd-networkd
Requires: %{_cross_os}systemd-resolved
%description -n %{_cross_os}netdog-systemd-networkd
%{summary}

%prep
%setup -T -c
%cargo_prep

%build
mkdir bin

echo "** Build Netdog for Wicked"
%cargo_build --manifest-path %{_builddir}/sources/Cargo.toml \
    -p netdog
    --features wicked
mv ${HOME}/.cache/%{__cargo_target}/release/netdog ${HOME}/.cache/%{__cargo_target}/release/netdog.wicked

echo "** Build Netdog for SystemD NetworkD"
%cargo_build --manifest-path %{_builddir}/sources/Cargo.toml \
    -p netdog
    --features systemd-networkd
mv ${HOME}/.cache/%{__cargo_target}/release/netdog ${HOME}/.cache/%{__cargo_target}/release/netdog.networkd


%install
install -p -m 0755 {HOME}/.cache/%{__cargo_target}/release/netdog.wicked %{buildroot}%{_cross_bindir}/netdog.wicked
install -p -m 0755 {HOME}/.cache/%{__cargo_target}/release/netdog.networkd %{buildroot}%{_cross_bindir}/netdog.networkd
install -p -m 0644 %{S:204} %{buildroot}%{_cross_tmpfilesdir}/netdog.conf

%files
%{_cross_attribution_vendor_dir}
%{_cross_licensedir}/COPYRIGHT
%{_cross_licensedir}/LICENSE-MIT
%{_cross_licensedir}/LICENSE-APACHE

%files -n %{_cross_os}netdog-wicked
%{_cross_bindir}/netdog.wicked
%{_cross_tmpfilesdir}/netdog.conf
%{_cross_unitdir}/generate-network-config.service
%{_cross_unitdir}/run-netdog.mount
%if %{with vmware_platform}
%{_cross_unitdir}/disable-udp-offload.service
%endif

%files -n %{_cross_os}netdog-systemd-networkd
%{_cross_bindir}/netdog.networkd
%{_cross_tmpfilesdir}/netdog.conf
%{_cross_unitdir}/generate-network-config.service
%{_cross_unitdir}/run-netdog.mount
%if %{with vmware_platform}
%{_cross_unitdir}/disable-udp-offload.service
%endif
%{_cross_unitdir}/write-network-status.service
%dir %{_cross_libdir}/systemd/resolved.conf.d
%{_cross_libdir}/systemd/resolved.conf.d/00-resolved.conf

%post -n %{_cross_os}netdog-wicked
ln -s %{_cross_bindir}/netdog.wicked %{_cross_bindir}/netdog

%post -n %{_cross_os}netdog-systemd-networkd
ln -s %{_cross_bindir}/netdog.networkd %{_cross_bindir}/netdog

%changelog
