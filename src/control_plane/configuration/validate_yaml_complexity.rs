use saphyr_parser::{Event, Parser};

pub(crate) const MAX_PROJECT_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_YAML_COLLECTION_ENTRIES: usize = 1_000;
const MAX_YAML_DEPTH: usize = 32;
const MAX_YAML_NODES: usize = 10_000;
const MAX_YAML_SCALAR_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum CollectionKind {
    Mapping,
    Sequence,
}

struct CollectionFrame {
    kind: CollectionKind,
    direct_nodes: usize,
}

/// Rejects expansion features and disproportionate YAML before deserialization.
pub(super) fn validate_yaml_complexity(source: &str) -> Result<(), String> {
    if source.len() > MAX_PROJECT_CONFIG_BYTES {
        return Err(format!(
            "YAML source is {} bytes; maximum is {MAX_PROJECT_CONFIG_BYTES} bytes",
            source.len()
        ));
    }

    let mut parser = Parser::new_from_str(source);
    let mut collections = Vec::new();
    let mut nodes = 0_usize;

    while let Some(event) = parser.next_event() {
        let (event, _) = event.map_err(|error| format!("invalid YAML: {error}"))?;
        match event {
            Event::Alias(_) => {
                return Err("YAML anchors and aliases are not supported".to_owned());
            }
            Event::Scalar(value, _, anchor, tag) => {
                reject_anchor_or_tag(anchor, tag.is_some())?;
                register_node(&mut collections, &mut nodes)?;
                if value.len() > MAX_YAML_SCALAR_BYTES {
                    return Err(format!("YAML scalar exceeds {MAX_YAML_SCALAR_BYTES} bytes"));
                }
            }
            Event::SequenceStart(anchor, tag) => {
                reject_anchor_or_tag(anchor, tag.is_some())?;
                register_node(&mut collections, &mut nodes)?;
                push_collection(&mut collections, CollectionKind::Sequence)?;
            }
            Event::MappingStart(anchor, tag) => {
                reject_anchor_or_tag(anchor, tag.is_some())?;
                register_node(&mut collections, &mut nodes)?;
                push_collection(&mut collections, CollectionKind::Mapping)?;
            }
            Event::SequenceEnd | Event::MappingEnd => {
                collections.pop();
            }
            Event::StreamEnd => break,
            Event::Nothing | Event::StreamStart | Event::DocumentStart(_) | Event::DocumentEnd => {}
        }
    }

    Ok(())
}

fn reject_anchor_or_tag(anchor: usize, tagged: bool) -> Result<(), String> {
    if anchor != 0 {
        return Err("YAML anchors and aliases are not supported".to_owned());
    }
    if tagged {
        return Err("YAML tags are not supported".to_owned());
    }

    Ok(())
}

fn register_node(collections: &mut [CollectionFrame], nodes: &mut usize) -> Result<(), String> {
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_YAML_NODES {
        return Err(format!("YAML document exceeds {MAX_YAML_NODES} nodes"));
    }

    let Some(parent) = collections.last_mut() else {
        return Ok(());
    };
    parent.direct_nodes = parent.direct_nodes.saturating_add(1);
    let entries = match parent.kind {
        CollectionKind::Mapping => parent.direct_nodes.div_ceil(2),
        CollectionKind::Sequence => parent.direct_nodes,
    };
    if entries > MAX_YAML_COLLECTION_ENTRIES {
        return Err(format!(
            "YAML collection exceeds {MAX_YAML_COLLECTION_ENTRIES} entries"
        ));
    }

    Ok(())
}

fn push_collection(
    collections: &mut Vec<CollectionFrame>,
    kind: CollectionKind,
) -> Result<(), String> {
    if collections.len().saturating_add(1) > MAX_YAML_DEPTH {
        return Err(format!("YAML nesting depth exceeds {MAX_YAML_DEPTH}"));
    }
    collections.push(CollectionFrame {
        kind,
        direct_nodes: 0,
    });

    Ok(())
}
