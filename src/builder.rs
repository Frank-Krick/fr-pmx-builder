use clap::error::Result;
use fr_logging::Logger;
use tonic::{transport::Channel, Request};

use crate::pmx::{
    factory::{
        channel_strip::{PmxChannelStrip, PmxChannelStripType},
        output_stage::PmxOutputStage,
        pmx_factory_client::PmxFactoryClient,
        CreateChannelStripRequest, CreateOutputStageRequest,
    },
    input::{PmxInput, PmxInputType},
    looper::PmxLooper,
    output::PmxOutput,
    pipewire::{
        node::ListNode, pipewire_client::PipewireClient, port::ListPort, CreateLinkByNameRequest,
        ListNodesRequest, ListPortsRequest,
    },
    pmx_registry_client::PmxRegistryClient,
    EmptyRequest, RegisterLooperRequest,
};

pub async fn get_inputs(
    mut client: PmxRegistryClient<Channel>,
    logger: &Logger,
) -> std::result::Result<Vec<PmxInput>, Box<dyn std::error::Error>> {
    logger.log_info("Reading inputs from registry");
    let request = Request::new(EmptyRequest {});
    let response = client.list_inputs(request).await?;
    Ok(response.into_inner().inputs)
}

pub async fn build_channel_strips(
    input_channels: &Vec<PmxInput>,
    mut client: PmxFactoryClient<Channel>,
    logger: &Logger,
) -> std::result::Result<Vec<PmxChannelStrip>, Box<dyn std::error::Error>> {
    logger.log_info("Creating channel strips");
    let mut channel_strips = Vec::new();
    for channel in input_channels {
        let request = Request::new(CreateChannelStripRequest {
            name: channel.name.clone(),
            channel_type: PmxChannelStripType::CrossFaded as i32,
        });
        let response = client.create_channel_strip(request).await?;
        let channel_strip = response.into_inner();
        channel_strips.push(channel_strip);
    }
    Ok(channel_strips)
}

pub async fn build_output_stage(
    mut client: PmxFactoryClient<Channel>,
    logger: &Logger,
) -> PmxOutputStage {
    logger.log_info("Creating output stage");
    let request = Request::new(CreateOutputStageRequest {
        name: String::from("Output Stage"),
    });
    let response = client.create_output_stage(request);
    response.await.unwrap().into_inner()
}

pub struct GroupChannelStrips {
    pub drums: PmxChannelStrip,
    pub bass: PmxChannelStrip,
    pub melody: PmxChannelStrip,
    pub atmos: PmxChannelStrip,
}

pub async fn build_group_channel_strips(
    client: PmxFactoryClient<Channel>,
    logger: &Logger,
) -> GroupChannelStrips {
    logger.log_info("Building group channels");
    let drums_channel =
        build_group_channel_strip(String::from("Drums"), client.clone(), logger).await;
    let bass_channel =
        build_group_channel_strip(String::from("Bass"), client.clone(), logger).await;
    let melody_channel =
        build_group_channel_strip(String::from("Melody"), client.clone(), logger).await;
    let atmos_channel =
        build_group_channel_strip(String::from("Atmos"), client.clone(), logger).await;
    GroupChannelStrips {
        drums: drums_channel,
        bass: bass_channel,
        melody: melody_channel,
        atmos: atmos_channel,
    }
}

async fn build_group_channel_strip(
    name: String,
    mut client: PmxFactoryClient<Channel>,
    logger: &Logger,
) -> PmxChannelStrip {
    logger.log_info(&format!("Creating group channel strip {name}"));
    let request = Request::new(CreateChannelStripRequest {
        name,
        channel_type: PmxChannelStripType::CrossFaded as i32,
    });
    let response = client.create_channel_strip(request).await.unwrap();
    response.into_inner()
}

pub async fn get_all_channel_strips(
    mut registry_client: PmxRegistryClient<Channel>,
) -> Vec<crate::pmx::channel_strip::PmxChannelStrip> {
    let request = Request::new(EmptyRequest {});
    let response = registry_client.list_channel_strips(request).await.unwrap();
    response.into_inner().channel_strips
}

pub async fn get_all_outputs(
    mut registry_client: PmxRegistryClient<Channel>,
) -> Vec<crate::pmx::output::PmxOutput> {
    let request = Request::new(EmptyRequest {});
    let response = registry_client.list_outputs(request).await.unwrap();
    response.into_inner().outputs
}

pub async fn connect_output_stage_to_outputs(
    output_stage: &PmxOutputStage,
    output_channels: &Vec<PmxOutput>,
    ports: &[ListPort],
    nodes: &[ListNode],
    plugins: &[crate::pmx::plugin::PmxPlugin],
    mut pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    let cross_fader_plugin = plugins
        .iter()
        .find(|p| p.id == output_stage.cross_fader_plugin_id);

    if let Some(cross_fader_plugin) = cross_fader_plugin {
        for output_channel in output_channels {
            if let (Some(left_path), Some(right_path)) = (
                output_channel.left_port_path.clone(),
                output_channel.right_port_path.clone(),
            ) {
                let left_port = ports.iter().find(|p| p.path == left_path);
                let right_port = ports.iter().find(|p| p.path == right_path);
                if let (Some(left_port), Some(right_port)) = (left_port, right_port) {
                    let left_node = nodes.iter().find(|n| n.object_serial == left_port.node_id);
                    let right_node = nodes.iter().find(|n| n.object_serial == right_port.node_id);

                    if let (Some(left_node), Some(right_node)) = (left_node, right_node) {
                        logger.log_info(&format!(
                            "Connecting {}:{} -> {}:{}",
                            cross_fader_plugin.name.clone(),
                            0,
                            left_node.name.clone(),
                            left_port.id,
                        ));

                        let request = Request::new(CreateLinkByNameRequest {
                            output_port_id: 0,
                            input_port_id: left_port.id,
                            output_node_name: cross_fader_plugin.name.clone(),
                            input_node_name: left_node.name.clone(),
                        });

                        pipewire_client.create_link_by_name(request).await.unwrap();

                        logger.log_info(&format!(
                            "Connecting {}:{} -> {}:{}",
                            cross_fader_plugin.name.clone(),
                            0,
                            right_node.name.clone(),
                            right_port.id,
                        ));

                        let request = Request::new(CreateLinkByNameRequest {
                            output_port_id: 0,
                            input_port_id: right_port.id,
                            output_node_name: cross_fader_plugin.name.clone(),
                            input_node_name: right_node.name.clone(),
                        });

                        pipewire_client.create_link_by_name(request).await.unwrap();
                    } else {
                        logger.log_info(&format!(
                            "Couldn't find nodes for ports: {:?}, {:?}",
                            left_port, right_port
                        ));
                    }
                } else {
                    logger.log_info(&format!(
                        "Couldn't find ports for paths: {}, {}",
                        left_path, right_path
                    ));
                }
            } else {
                logger.log_info(&format!(
                    "Channel doesn't have both path filled {:?}",
                    output_channel
                ));
            }
        }
    }
}

pub async fn connect_group_channel_strips_to_output_stage_channels(
    group_channel_strips: &GroupChannelStrips,
    output_stage: &PmxOutputStage,
    plugins: &[crate::pmx::plugin::PmxPlugin],
    channel_strips: &[crate::pmx::channel_strip::PmxChannelStrip],
    pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    let left_channel_strip = channel_strips
        .iter()
        .find(|c| c.id == output_stage.left_channel_strip_id);

    let right_channel_strip = channel_strips
        .iter()
        .find(|c| c.id == output_stage.right_channel_strip_id);

    if let (Some(left_channel_strip), Some(right_channel_strip)) =
        (left_channel_strip, right_channel_strip)
    {
        let left_plugin = plugins
            .iter()
            .find(|p| p.id == left_channel_strip.saturator_plugin_id);

        let right_plugin = plugins
            .iter()
            .find(|p| p.id == right_channel_strip.saturator_plugin_id);

        if let (Some(left_plugin), Some(right_plugin)) = (left_plugin, right_plugin) {
            let drum_gain_plugin = plugins
                .iter()
                .find(|p| p.id == group_channel_strips.drums.gain_plugin_id);
            let bass_gain_plugin = plugins
                .iter()
                .find(|p| p.id == group_channel_strips.bass.gain_plugin_id);
            let melody_gain_plugin = plugins
                .iter()
                .find(|p| p.id == group_channel_strips.melody.gain_plugin_id);
            let atmos_gain_plugin = plugins
                .iter()
                .find(|p| p.id == group_channel_strips.atmos.gain_plugin_id);

            if let Some(drum_gain_plugin) = drum_gain_plugin {
                connect_plugins(
                    drum_gain_plugin,
                    left_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;

                connect_plugins(
                    drum_gain_plugin,
                    right_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;
            } else {
                logger.log_info("Couldn't find drum gain plugin");
            }

            if let Some(bass_gain_plugin) = bass_gain_plugin {
                connect_plugins(
                    bass_gain_plugin,
                    left_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;

                connect_plugins(
                    bass_gain_plugin,
                    right_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;
            } else {
                logger.log_info("Couldn't find bass gain plugin");
            }

            if let Some(melody_gain_plugin) = melody_gain_plugin {
                connect_plugins(
                    melody_gain_plugin,
                    left_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;

                connect_plugins(
                    melody_gain_plugin,
                    right_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;
            } else {
                logger.log_info("Couldn't find melody gain plugin");
            }

            if let Some(atmos_gain_plugin) = atmos_gain_plugin {
                connect_plugins(
                    atmos_gain_plugin,
                    left_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;

                connect_plugins(
                    atmos_gain_plugin,
                    right_plugin,
                    &[(0, 0), (1, 1)],
                    pipewire_client.clone(),
                    logger,
                )
                .await;
            } else {
                logger.log_info("Couldn't find atmos gain plugin");
            }
        } else {
            logger.log_info(&format!(
                "Couldn't find a plugin left: {:?}, right: {:?}",
                left_plugin, right_plugin
            ));
        }
    } else {
        logger.log_info(&format!(
            "Couldn't find a channel strip left: {:?}, right: {:?}",
            left_channel_strip, right_channel_strip
        ));
    }
}

async fn connect_plugins(
    output_plugin: &crate::pmx::plugin::PmxPlugin,
    input_plugin: &crate::pmx::plugin::PmxPlugin,
    connections: &[(u32, u32)],
    mut pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    for connection in connections {
        logger.log_info(&format!(
            "Connecting {}:{} -> {}:{}",
            output_plugin.name, connection.0, input_plugin.name, connection.1
        ));
        let request = Request::new(CreateLinkByNameRequest {
            output_port_id: connection.0,
            input_port_id: connection.1,
            output_node_name: output_plugin.name.clone(),
            input_node_name: input_plugin.name.clone(),
        });
        pipewire_client.create_link_by_name(request).await.unwrap();
    }
}

pub async fn connect_channel_strips_to_group_channel_strips(
    input_channels: &[PmxInput],
    channel_strips: &[PmxChannelStrip],
    group_channel_strips: &GroupChannelStrips,
    plugins: &[crate::pmx::plugin::PmxPlugin],
    pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    for input_channel in input_channels {
        let group_name = &input_channel.group_channel_strip_name;

        let mut group_channel_strip = None;
        if group_name == "Drums" {
            group_channel_strip = Some(&group_channel_strips.drums)
        } else if group_name == "Bass" {
            group_channel_strip = Some(&group_channel_strips.bass)
        } else if group_name == "Melody" {
            group_channel_strip = Some(&group_channel_strips.melody)
        } else if group_name == "Atmos" {
            group_channel_strip = Some(&group_channel_strips.atmos)
        }

        let channel_strip = channel_strips.iter().find(|c| c.name == input_channel.name);

        if let Some((group_channel_strip, input_channel_strip)) =
            group_channel_strip.zip(channel_strip)
        {
            let group_channel_plugin = plugins
                .iter()
                .find(|p| p.id == group_channel_strip.saturator_plugin_id);
            let input_channel_plugin = plugins
                .iter()
                .find(|p| p.id == input_channel_strip.gain_plugin_id);

            if let Some((group_channel_plugin, input_channel_plugin)) =
                group_channel_plugin.zip(input_channel_plugin)
            {
                logger.log_info(&format!(
                    "Connecting {}:{} -> {}:{}",
                    input_channel_plugin.name, 0, group_channel_plugin.name, 0
                ));
                let request = Request::new(CreateLinkByNameRequest {
                    output_port_id: 0,
                    input_port_id: 0,
                    output_node_name: input_channel_plugin.name.clone(),
                    input_node_name: group_channel_plugin.name.clone(),
                });
                pipewire_client
                    .clone()
                    .create_link_by_name(request)
                    .await
                    .unwrap();

                logger.log_info(&format!(
                    "Connecting {}:{} -> {}:{}",
                    input_channel_plugin.name, 1, group_channel_plugin.name, 1
                ));
                let request = Request::new(CreateLinkByNameRequest {
                    output_port_id: 1,
                    input_port_id: 1,
                    output_node_name: input_channel_plugin.name.clone(),
                    input_node_name: group_channel_plugin.name.clone(),
                });
                pipewire_client
                    .clone()
                    .create_link_by_name(request)
                    .await
                    .unwrap();
            } else {
                logger.log_info("Couldn't find plugin");
                continue;
            }
        } else {
            logger.log_info(&format!(
                "Couldn't find group channel {} for input channel {}",
                group_name, input_channel.name
            ));
            continue;
        }
    }
}

pub async fn connect_inputs_to_channel_strips(
    input_channels: &Vec<PmxInput>,
    channel_strips: &Vec<PmxChannelStrip>,
    plugins: &[crate::pmx::plugin::PmxPlugin],
    ports: &[ListPort],
    nodes: &[ListNode],
    pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    logger.log_info("Connecting inputs to channel strips");

    logger.log_info(&format!(
        "Found {} ports and {} nodes",
        ports.len(),
        nodes.len()
    ));

    let pairs = std::iter::zip(input_channels, channel_strips);

    for (input, channel) in pairs {
        logger.log_info(&format!(
            "Connecting input {} to channel {}",
            input.name, channel.name
        ));

        if input.input_type == PmxInputType::None as i32 {
            logger.log_info("Input type is None, nothing to do");
            continue;
        };

        let left_port = ports
            .iter()
            .find(|p| p.path == input.left_port_path.clone().unwrap());

        let plugin = plugins
            .iter()
            .find(|p| p.id == channel.cross_fader_plugin_id.unwrap());

        match (left_port, plugin) {
            (Some(port), Some(plugin)) => {
                if let Some(node) = nodes.iter().find(|n| n.object_serial == port.node_id) {
                    let mut pipewire_client = pipewire_client.clone();

                    logger.log_info(&format!(
                        "Connecting {}:{} -> {}:{}",
                        node.name.clone(),
                        port.id,
                        plugin.name.clone(),
                        0
                    ));

                    let request = Request::new(CreateLinkByNameRequest {
                        output_port_id: port.id,
                        input_port_id: 0,
                        output_node_name: node.name.clone(),
                        input_node_name: plugin.name.clone(),
                    });
                    pipewire_client.create_link_by_name(request).await?;
                } else {
                    logger.log_info("Couldn't find node for port");
                }
            }
            (None, None) => {
                logger.log_info("Can't connect, port and plugin not found");
            }
            (None, Some(_)) => {
                logger.log_info("Can't connect, port not found");
            }
            (Some(_), None) => {
                logger.log_info("Can't connect, plugin not found");
            }
        };

        if input.input_type == PmxInputType::StereoInput as i32 {
            let right_port = ports
                .iter()
                .find(|p| p.path == input.right_port_path.clone().unwrap());

            match (right_port, plugin) {
                (Some(port), Some(plugin)) => {
                    if let Some(node) = nodes.iter().find(|n| n.object_serial == port.node_id) {
                        logger.log_info(&format!(
                            "Connecting {}:{} -> {}:{}",
                            node.name.clone(),
                            port.id,
                            plugin.name.clone(),
                            0
                        ));

                        let mut pipewire_client = pipewire_client.clone();
                        let request = Request::new(CreateLinkByNameRequest {
                            output_port_id: port.id,
                            input_port_id: 1,
                            output_node_name: node.name.clone(),
                            input_node_name: plugin.name.clone(),
                        });
                        pipewire_client.create_link_by_name(request).await?;
                    }
                }
                (None, None) => (),
                (None, Some(_)) => (),
                (Some(_), None) => (),
            };
        }
    }

    Ok(())
}

pub async fn get_nodes(
    mut pipewire_client: PipewireClient<Channel>,
) -> std::result::Result<Vec<super::pmx::pipewire::node::ListNode>, Box<dyn std::error::Error>> {
    let nodes_request = Request::new(ListNodesRequest {});
    let nodes_response = pipewire_client.list_nodes(nodes_request).await?;
    Ok(nodes_response.into_inner().nodes)
}

pub async fn get_plugins(
    mut registry_client: PmxRegistryClient<Channel>,
) -> std::result::Result<Vec<super::pmx::plugin::PmxPlugin>, Box<dyn std::error::Error>> {
    let plugin_request = Request::new(EmptyRequest {});
    let plugin_response = registry_client.list_plugins(plugin_request).await?;
    Ok(plugin_response.into_inner().plugins)
}

pub async fn get_ports(
    mut pipewire_client: PipewireClient<Channel>,
) -> std::result::Result<Vec<super::pmx::pipewire::port::ListPort>, Box<dyn std::error::Error>> {
    let port_request = Request::new(ListPortsRequest {
        node_id_filter: None,
    });
    let port_response = pipewire_client.list_ports(port_request).await?;
    Ok(port_response.into_inner().ports)
}

pub async fn register_loopers_for_input_channels(
    input_channels: &[PmxInput],
    registry_client: PmxRegistryClient<Channel>,
) -> Vec<PmxLooper> {
    let mut result = Vec::new();
    for (index, _channel) in input_channels.iter().enumerate() {
        let looper = register_looper(index as u32, registry_client.clone())
            .await
            .unwrap();
        result.push(looper);
    }
    result
}

pub async fn connect_loopers_to_channel_strips(
    loopers: &[PmxLooper],
    channel_strips: &Vec<PmxChannelStrip>,
    plugins: &[crate::pmx::plugin::PmxPlugin],
    pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    for (looper, channel_strip) in std::iter::zip(loopers, channel_strips) {
        connect_looper_to_channel_strip(
            looper,
            channel_strip,
            plugins,
            pipewire_client.clone(),
            logger,
        )
        .await
        .unwrap();
    }
}

async fn connect_looper_to_channel_strip(
    looper: &PmxLooper,
    channel_strip: &PmxChannelStrip,
    plugins: &[crate::pmx::plugin::PmxPlugin],
    mut pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if channel_strip.channel_type() == PmxChannelStripType::Basic {
        logger.log_info("Channel strip type is Basic, nothing to do!");
        return Ok(());
    }

    if let Some(plugin) = plugins
        .iter()
        .find(|p| p.id == channel_strip.cross_fader_plugin_id.unwrap())
    {
        logger.log_info(&format!(
            "Connecting {}:{} -> {}:{}",
            String::from("sooperlooper"),
            looper.loop_number + 2,
            plugin.name.clone(),
            2
        ));

        let request = Request::new(CreateLinkByNameRequest {
            output_port_id: looper.loop_number + 2,
            input_port_id: 2,
            output_node_name: String::from("sooperlooper"),
            input_node_name: plugin.name.clone(),
        });
        pipewire_client.create_link_by_name(request).await?;

        logger.log_info(&format!(
            "Connecting {}:{} -> {}:{}",
            String::from("sooperlooper"),
            looper.loop_number + 3,
            plugin.name.clone(),
            3
        ));

        let request = Request::new(CreateLinkByNameRequest {
            output_port_id: looper.loop_number + 3,
            input_port_id: 3,
            output_node_name: String::from("sooperlooper"),
            input_node_name: plugin.name.clone(),
        });
        pipewire_client.create_link_by_name(request).await?;
    };

    Ok(())
}

pub async fn connect_loopers_to_inputs(
    inputs: &[PmxInput],
    loopers: &[PmxLooper],
    nodes: &[ListNode],
    ports: &[ListPort],
    pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    let channel_and_looper_pairs = std::iter::zip(inputs, loopers);
    for (channel, looper) in channel_and_looper_pairs {
        connect_looper_to_input(
            channel,
            looper,
            ports,
            nodes,
            pipewire_client.clone(),
            logger,
        )
        .await;
    }
}

pub async fn connect_looper_to_input(
    input: &PmxInput,
    looper: &PmxLooper,
    ports: &[ListPort],
    nodes: &[ListNode],
    mut pipewire_client: PipewireClient<Channel>,
    logger: &Logger,
) {
    logger.log_info(&format!(
        "Connecting input {} to looper {}",
        input.name, looper.loop_number,
    ));

    if input.input_type() == PmxInputType::None {
        logger.log_info("Input type is None, nothing to do");
        return;
    }

    if let Some(port) = ports
        .iter()
        .find(|p| p.path == input.left_port_path.clone().unwrap())
    {
        if let Some(node) = nodes.iter().find(|n| n.object_serial == port.node_id) {
            logger.log_info(&format!(
                "Connecting {}:{} -> {}:{}",
                node.name.clone(),
                0,
                String::from("sooperlooper"),
                looper.loop_number + 2,
            ));

            let request = Request::new(CreateLinkByNameRequest {
                output_port_id: 2 * looper.loop_number + 2,
                input_port_id: 0,
                output_node_name: String::from("sooperlooper"),
                input_node_name: node.name.clone(),
            });
            pipewire_client.create_link_by_name(request).await.unwrap();
        }
    }

    if input.input_type() == PmxInputType::MonoInput {
        return;
    }

    if let Some(port) = ports
        .iter()
        .find(|p| p.path == input.right_port_path.clone().unwrap())
    {
        if let Some(node) = nodes.iter().find(|n| n.object_serial == port.node_id) {
            logger.log_info(&format!(
                "Connecting {}:{} -> {}:{}",
                node.name.clone(),
                1,
                String::from("sooperlooper"),
                looper.loop_number + 3,
            ));

            let request = Request::new(CreateLinkByNameRequest {
                output_port_id: 2 * looper.loop_number + 3,
                input_port_id: 1,
                output_node_name: String::from("sooperlooper"),
                input_node_name: node.name.clone(),
            });
            pipewire_client.create_link_by_name(request).await.unwrap();
        }
    }
}

async fn register_looper(
    loop_number: u32,
    mut registry_client: PmxRegistryClient<Channel>,
) -> Result<PmxLooper, Box<dyn std::error::Error>> {
    let looper_request = Request::new(RegisterLooperRequest { loop_number });
    Ok(registry_client
        .register_looper(looper_request)
        .await
        .unwrap()
        .into_inner())
}
