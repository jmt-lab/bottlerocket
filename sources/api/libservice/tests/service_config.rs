use libservice::template::RenderDestination;
use libservice::ServiceConfigurations;
use std::collections::BTreeSet;
use std::path::PathBuf;

const TESTCASE_PATH: &str = "tests/testcases";

async fn load_services(testcase: &str) -> ServiceConfigurations {
    let testcase_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(TESTCASE_PATH)
        .join(testcase);
    ServiceConfigurations::from_filesystem(&testcase_path)
        .await
        .unwrap()
}

#[tokio::test]
async fn testcase_simple() {
    let service_configs = load_services("simple").await;

    let all_services: Vec<_> = service_configs.services().collect();
    assert_eq!(all_services.len(), 1);
    assert_eq!(all_services[0].name, "sample");
    assert_eq!(all_services[0].restart_commands, ["restart", "me"]);

    let all_configs: Vec<_> = service_configs.configuration_templates().collect();
    assert_eq!(all_configs.len(), 1);

    let affected_configs: Vec<_> = service_configs
        .configurations_affected_by_setting("sample-extension")
        .collect();

    assert_eq!(affected_configs.len(), 1);
    assert_eq!(affected_configs[0], all_configs[0]);

    let affected_services: Vec<_> = service_configs
        .services_affected_by_config_template(&affected_configs[0])
        .collect();

    assert_eq!(affected_services.len(), 1);
    assert_eq!(affected_services[0], all_services[0]);
}

#[tokio::test]
async fn testcase_multi() {
    let service_configs = load_services("multi").await;

    // Check that all expected services were loaded.
    let all_services: Vec<_> = service_configs.services().collect();
    assert_eq!(all_services.len(), 2);

    let sample1_service = all_services.iter().find(|i| i.name == "sample1").unwrap();
    assert_eq!(sample1_service.restart_commands, ["restart", "sample1"]);

    let sample2_service = all_services.iter().find(|i| i.name == "sample2").unwrap();
    assert_eq!(sample2_service.restart_commands, ["restart", "sample2"]);

    // Check that all expected config templates were loaded.
    let all_configs: Vec<_> = service_configs.configuration_templates().collect();

    assert_eq!(all_configs.len(), 2);
    let sample1_config = all_configs
        .iter()
        .find(|i| i.template_filepath.file_name().unwrap() == "sample1.template")
        .unwrap();
    assert_eq!(
        sample1_config
            .affected_services
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        [
            sample1_service.filepath.clone(),
            sample2_service.filepath.clone()
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        sample1_config
            .render_destinations
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        vec![
            RenderDestination {
                path: "/etc/sample1.conf".into(),
                mode: "0644".to_string(),
                user: None,
                group: None,
            },
            RenderDestination {
                path: "/etc/another-path.conf".into(),
                mode: "0700".to_string(),
                user: Some("user".to_string()),
                group: Some("group".to_string()),
            },
            RenderDestination {
                path: "/etc/another-sample1.conf".into(),
                mode: "0644".to_string(),
                user: None,
                group: None,
            },
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );

    let sample2_config = all_configs
        .iter()
        .find(|i| i.template_filepath.file_name().unwrap() == "sample2.template")
        .unwrap();

    assert_eq!(
        sample2_config
            .affected_services
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        [sample1_service.filepath.clone(),]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        sample2_config
            .render_destinations
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        vec![RenderDestination {
            path: "/etc/sample2.conf".into(),
            mode: "0644".to_string(),
            user: None,
            group: None,
        },]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );

    // =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=
    // Check that queries result as expected.
    let affected_configs = service_configs
        .configurations_affected_by_setting("std")
        .collect::<BTreeSet<_>>();
    assert_eq!(affected_configs.len(), all_configs.len());
    assert_eq!(affected_configs, all_configs.iter().cloned().collect());

    // =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=
    let affected_configs = service_configs
        .configurations_affected_by_setting("ext1")
        .collect::<Vec<_>>();
    assert_eq!(affected_configs.len(), 1);
    assert_eq!(&affected_configs[0], sample1_config);

    let affected_services = service_configs
        .services_affected_by_config_template(&sample1_config)
        .collect::<BTreeSet<_>>();
    assert_eq!(affected_services.len(), all_services.len());
    assert_eq!(affected_services, all_services.iter().cloned().collect());

    // =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=
    let affected_configs = service_configs
        .configurations_affected_by_setting("ext2")
        .collect::<Vec<_>>();
    assert_eq!(affected_configs.len(), 1);
    assert_eq!(&affected_configs[0], sample2_config);

    let affected_services = service_configs
        .services_affected_by_config_template(&sample2_config)
        .collect::<Vec<_>>();
    assert_eq!(affected_services.len(), 1);
    assert_eq!(&affected_services[0], sample1_service);

    // =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=  =^..^=
    let affected_configs = service_configs
        .configurations_affected_by_setting("no-such-extension")
        .collect::<Vec<_>>();
    assert_eq!(affected_configs.len(), 0);
}
