use std::{fs, path::Path};

use anyhow::{Context, Result};
use ts_rs::{Config, TS};

use crate::{
    api::{Enqueue, UpdateVoice},
    player::{EndReason, PlaybackState, QueueItem, Snapshot, VoiceState},
    resolver::Resolver,
};

pub(crate) fn export() -> Result<()> {
    let config = Config::new();
    let declarations = [
        Resolver::decl(&config),
        Enqueue::decl(&config),
        QueueItem::decl(&config),
        UpdateVoice::decl(&config),
        VoiceState::decl(&config),
        PlaybackState::decl(&config),
        Snapshot::decl(&config),
        EndReason::decl(&config),
    ];
    let mut source = String::from("// Automatically generated types. DO NOT EDIT!\n\n");
    for declaration in declarations {
        source.push_str("export ");
        source.push_str(&declaration);
        source.push_str("\n\n");
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("sdks/typescript/src/generated.ts");
    fs::write(path, source).context("failed to write TypeScript bindings")
}
