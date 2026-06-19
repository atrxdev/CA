use crate::game::map::map_asset::Map;
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use thiserror::Error;

#[derive(Default, bevy::reflect::TypePath)]
pub struct MapAssetLoader;

#[derive(Debug, Error)]
pub enum MapAssetLoaderError {
    #[error("Could not load map: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not parse map RON: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for MapAssetLoader {
    type Asset = Map;
    type Settings = ();
    type Error = MapAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let custom_asset = ron::de::from_bytes::<Map>(&bytes)?;
        Ok(custom_asset)
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}
