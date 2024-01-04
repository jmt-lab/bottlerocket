%global _cross_first_party 1
%undefine _debugsoure_packages

Name: %{_cross_os}netdog-wicked
Version: 0.1.1
Release: 0%{?dist}
Summary: Bottlerocket network configuration helper
License: Apache-2.0 OR MIT
URL: https://github.com/bottlerocket-os/bottlerocket

Source2: ../netdog/run-netdog.mount
Source3: ../netdog/write-network-status.service
Source4: ../netdog/netdog-tmpfiles.conf
Source5: ../netdog/disable-udp-offload.service

BuildRequires: %{_cross_os}glibc-devel
Requires: %{_cross_os}netdog
Requires: %{_cross_os}wicked
%description
%{summary}

%prep
%setup -T -c
%cargo_prep

%build
mkdir bin

echo "** Build Netdog for Wicked"
%cargo_build --manifest-path %{_builddir}/sources/Cargo.toml \
    -p netdog \
    --features wicked

%install
install -d %{buildroot}%{_cross_bindir}
install -d %{buildroot}%{_cross_tmpfilesdir}
install -p -m 0755 ${HOME}/.cache/%{__cargo_target}/release/netdog %{buildroot}%{_cross_bindir}/netdog
install -p -m 0644 %{S:4} %{buildroot}%{_cross_tmpfilesdir}/netdog.conf

%files
%{_cross_attribution_vendor_dir}
%{_cross_licensedir}/COPYRIGHT
%{_cross_licensedir}/LICENSE-MIT
%{_cross_licensedir}/LICENSE-APACHE
%{_cross_bindir}/netdog.wicked
%{_cross_tmpfilesdir}/netdog.conf
%{_cross_unitdir}/generate-network-config.service
%{_cross_unitdir}/run-netdog.mount
%if %{with vmware_platform}
%{_cross_unitdir}/disable-udp-offload.service
%endif

%changelog
