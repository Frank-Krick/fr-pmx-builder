mod builder;

pub mod pmx {
    tonic::include_proto!("pmx");

    pub mod mod_host {
        tonic::include_proto!("pmx.mod_host");

        pub mod plugins {
            tonic::include_proto!("pmx.mod_host.plugins");
        }
    }

    pub mod input {
        tonic::include_proto!("pmx.input");
    }

    pub mod output {
        tonic::include_proto!("pmx.output");
    }

    pub mod plugin {
        tonic::include_proto!("pmx.plugin");
    }

    pub mod channel_strip {
        tonic::include_proto!("pmx.channel_strip");
    }

    pub mod looper {
        tonic::include_proto!("pmx.looper");
    }

    pub mod output_stage {
        tonic::include_proto!("pmx.output_stage");
    }

    pub mod pipewire {
        tonic::include_proto!("pmx.pipewire");

        pub mod node {
            tonic::include_proto!("pmx.pipewire.node");
        }

        pub mod port {
            tonic::include_proto!("pmx.pipewire.port");
        }

        pub mod application {
            tonic::include_proto!("pmx.pipewire.application");
        }

        pub mod device {
            tonic::include_proto!("pmx.pipewire.device");
        }

        pub mod link {
            tonic::include_proto!("pmx.pipewire.link");
        }
    }

    pub mod factory {
        tonic::include_proto!("pmx.factory");

        pub mod channel_strip {
            tonic::include_proto!("pmx.factory.channel_strip");
        }

        pub mod output_stage {
            tonic::include_proto!("pmx.factory.output_stage");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    fr_logging::setup_logging();
    let (logger_sender, logger_receiver) = tokio::sync::mpsc::unbounded_channel();
    let logger_factory = fr_logging::LoggerFactory::new(logger_sender);

    let logger = logger_factory.new_logger(String::from("fr_pmx_builder"));

    tokio::join!(
        build_pmx(logger),
        fr_logging::run_logging_task(logger_receiver)
    )
    .0
}

async fn build_pmx(logger: fr_logging::Logger) -> Result<(), Box<dyn std::error::Error>> {
    let service_urls = fr_pmx_config_lib::read_service_urls();
    let registry_client =
        pmx::pmx_registry_client::PmxRegistryClient::connect(service_urls.pmx_registry_url).await?;
    let factory_client =
        pmx::factory::pmx_factory_client::PmxFactoryClient::connect(service_urls.pmx_factory_url)
            .await?;
    let pipewire_client =
        pmx::pipewire::pipewire_client::PipewireClient::connect(service_urls.pipewire_registry_url)
            .await?;

    let input_channels = builder::get_inputs(registry_client.clone(), &logger).await?;
    let channel_strips =
        builder::build_channel_strips(&input_channels, factory_client.clone(), &logger).await?;

    let plugins = builder::get_plugins(registry_client.clone()).await?;
    let ports = builder::get_ports(pipewire_client.clone()).await?;
    let nodes = builder::get_nodes(pipewire_client.clone()).await?;

    builder::connect_inputs_to_channel_strips(
        &input_channels,
        &channel_strips,
        &plugins,
        &ports,
        &nodes,
        pipewire_client.clone(),
        &logger,
    )
    .await?;

    let loopers =
        builder::register_loopers_for_input_channels(&input_channels, registry_client.clone())
            .await;

    builder::connect_loopers_to_inputs(
        &input_channels,
        &loopers,
        &nodes,
        &ports,
        pipewire_client.clone(),
        &logger,
    )
    .await;

    builder::connect_loopers_to_channel_strips(
        &loopers,
        &channel_strips,
        &plugins,
        pipewire_client.clone(),
        &logger,
    )
    .await;

    let group_channel_strips =
        builder::build_group_channel_strips(factory_client.clone(), &logger).await;

    let plugins = builder::get_plugins(registry_client.clone()).await?;

    builder::connect_channel_strips_to_group_channel_strips(
        &input_channels,
        &channel_strips,
        &group_channel_strips,
        &plugins,
        pipewire_client.clone(),
        &logger,
    )
    .await;

    let output_stage = builder::build_output_stage(factory_client.clone(), &logger).await;

    let plugins = builder::get_plugins(registry_client.clone()).await?;

    let channel_strips = builder::get_all_channel_strips(registry_client.clone()).await;

    builder::connect_group_channel_strips_to_output_stage_channels(
        &group_channel_strips,
        &output_stage,
        &plugins,
        &channel_strips,
        pipewire_client.clone(),
        &logger,
    )
    .await;

    let output_channels = builder::get_all_outputs(registry_client.clone()).await;

    builder::connect_output_stage_to_outputs(
        &output_stage,
        &output_channels,
        &ports,
        &nodes,
        &plugins,
        pipewire_client.clone(),
        &logger,
    )
    .await;

    Ok(())
}
