use quantix_lib::{
    inspect_quantix_host, HostCommandInterface, HostConnectionState, HostRuntime, QuantixHost,
    QuantixHostStatus, RendererAssetSource,
};

#[test]
fn engineer_can_inspect_host_readiness_through_the_public_command() {
    let application_home = tempfile::tempdir().expect("temporary application home");
    let host = QuantixHost::new(application_home.path());

    let status = inspect_quantix_host(&host);

    assert_eq!(
        status,
        QuantixHostStatus {
            connection: HostConnectionState::Connected,
            runtime: HostRuntime::LocalTauriDesktop,
            command_interface: HostCommandInterface::NamedDomainCommands,
            renderer_assets: RendererAssetSource::BundledLocal,
        }
    );
}
