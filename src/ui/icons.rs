//! Tiny pixel-art icons drawn from ASCII maps in code, shown crisp at a whole-number scale.

use crate::island::TileKind;
use crate::keeper::{Building, OrderKind};
use crate::palette;
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

const TREE: &[&str] = &[
    ".....oo.....",
    "....oggo....",
    "...ogggGo...",
    "..oggggGGo..",
    "...ogggGo...",
    "..oggggGGo..",
    ".ogggggGGGo.",
    "oggggggGGGGo",
    "ooooobbooooo",
    "....obbo....",
    "....obbo....",
    "....oooo....",
];

const TOWER: &[&str] = &[
    ".....off....",
    ".....o......",
    "....orRo....",
    "...orrRRo...",
    "..orrrRRRo..",
    ".orrrrRRRRo.",
    "..owwwwwwo..",
    "..oWo..oWo..",
    "..oWo..oWo..",
    "..oWwwwwWo..",
    ".osssssssso.",
    ".oooooooooo.",
];

const CATAPULT: &[&str] = &[
    ".......ooo..",
    "......ossSo.",
    "......osSSo.",
    ".....owooo..",
    "....owo.....",
    "...owo...o..",
    "..owo...owo.",
    ".owo....owo.",
    "oooooooooooo",
    "owwwwwwwwwwo",
    ".oBBo..oBBo.",
    "..oo....oo..",
];

const REPAIR: &[&str] = &[
    "............",
    "..ooooooo...",
    ".oiiiiiiio..",
    ".oicciiiio..",
    ".oiiiiiiio..",
    "..oooHooo...",
    "....oHHo..f.",
    "....oHHo.fff",
    "....oHHo..f.",
    "....oHHo....",
    "....oHHo....",
    "....oooo....",
];

const BOOT: &[&str] = &[
    "............",
    "...oooo.....",
    "...obbo.....",
    "...obbo.....",
    "...olbo.....",
    "...obbo.....",
    "...obbboo...",
    "...obbbbbo..",
    "...obbbbbbo.",
    "...oBBBBBBo.",
    "...oooooooo.",
    "............",
];

const LOG: &[&str] = &[
    "............",
    "............",
    "............",
    "oooooooooooo",
    "obbbbbbboeeo",
    "obbBbbbboeEo",
    "obbbbbBboEeo",
    "oBBBBBBBoeeo",
    "oooooooooooo",
    "............",
    "............",
    "............",
];

const BELL: &[&str] = &[
    ".....oo.....",
    "....oyyo....",
    "...oycyYo...",
    "..oycyyYYo..",
    "..oyyyyYYo..",
    "..oyyyyYYo..",
    ".oyyyyyyYYo.",
    ".oyyyyyyYYo.",
    "oyyyyyyyyYYo",
    "oooooooooooo",
    ".....oYo....",
    "......o.....",
];

const HOURGLASS: &[&str] = &[
    "oooooooooooo",
    "owwwwwwwwwwo",
    "oooooooooooo",
    ".occcccccco.",
    "..odddddco..",
    "...oddco....",
    "....oddo....",
    "...ocddco...",
    "..occddcco..",
    ".oddddddddo.",
    "oooooooooooo",
    "owwwwwwwwwwo",
];

const SLIME: &[&str] = &[
    "............",
    "............",
    "....oooo....",
    "..oommmmoo..",
    ".omcmmmmmmo.",
    ".ommmmmmmmo.",
    "ommkmmmmkmmo",
    "ommkmmmmkmmo",
    "ommmmmmmmmmo",
    "oMMMMMMMMMMo",
    ".oooooooooo.",
    "............",
];

const COIN: &[&str] = &[
    "............",
    "...oooooo...",
    "..onnnnnno..",
    ".oncnnnnNNo.",
    ".onnnNNnnNo.",
    ".onnNnnNnNo.",
    ".onnNnnNnNo.",
    ".onnnNNnnNo.",
    ".onnnnnnNNo.",
    "..oNNNNNNo..",
    "...oooooo...",
    "............",
];

const HEARTH: &[&str] = &[
    ".....o......",
    "....oFo.....",
    "....oFFo....",
    "...oFCFo.o..",
    "..oFFCCFoFo.",
    "..oFCCCCFFo.",
    ".oFCCnnCCFo.",
    ".oFCnnnnCFo.",
    "..oFCnnCFo..",
    "oooooooooooo",
    "osssSsssSsso",
    "oooooooooooo",
];

// Tiles, seen from above with a strip of dirt along the front edge.
const FOREST_TILE: &[&str] = &[
    "oooooooooooo",
    "ohGGGhhhhhho",
    "oGgggGhGGGho",
    "oGgggGGgggGo",
    "ohGGGhGgggGo",
    "ohhbhhhGGGho",
    "ohhhGGGhbhho",
    "ohhGgggGhhho",
    "ohhGgggGhhho",
    "ohhhGGGhhhho",
    "ouuuubuuuuuo",
    "oooooooooooo",
];

const MEADOW_TILE: &[&str] = &[
    "oooooooooooo",
    "oaaaaaaaaaao",
    "oaAaaaapaaao",
    "oaaaaaaaaaAo",
    "oaaavaaaaaao",
    "oaaaaaaAaaao",
    "oapaaaaaaaao",
    "oaaaaaAaavao",
    "oaaAaaaaaaao",
    "oaaaaaaaaaao",
    "ouuuuuuuuuuo",
    "oooooooooooo",
];

const ROAD_TILE: &[&str] = &[
    "oooooooooooo",
    "oaaaddddaaao",
    "oaAaddddaaao",
    "oaaadddDaaAo",
    "oaaaddddaaao",
    "oaaaDdddaaao",
    "oapaddddaaao",
    "oaaaddddaaao",
    "oaAaddDdaaao",
    "oaaaddddaaao",
    "ouuuUUUUuuuo",
    "oooooooooooo",
];

/// Icon handles for the UI.
#[derive(Resource)]
pub struct Icons {
    pub chop: Handle<Image>,
    pub build: Handle<Image>,
    pub walk: Handle<Image>,
    pub catapult: Handle<Image>,
    pub repair: Handle<Image>,
    pub wood: Handle<Image>,
    pub bell: Handle<Image>,
    pub hourglass: Handle<Image>,
    pub slime: Handle<Image>,
    pub coin: Handle<Image>,
    pub hearth: Handle<Image>,
    pub forest_tile: Handle<Image>,
    pub meadow_tile: Handle<Image>,
    pub road_tile: Handle<Image>,
}

impl Icons {
    /// The icon on an order's card.
    pub fn order(&self, kind: &OrderKind) -> Handle<Image> {
        match kind {
            OrderKind::Chop(_) => self.chop.clone(),
            OrderKind::Build(_, building) => self.building(*building),
            OrderKind::Repair(_) => self.repair.clone(),
            OrderKind::Man(_) => self.catapult.clone(),
            OrderKind::Walk(_) => self.walk.clone(),
        }
    }

    pub fn building(&self, building: Building) -> Handle<Image> {
        match building {
            Building::Tower => self.build.clone(),
            Building::Catapult => self.catapult.clone(),
        }
    }

    pub fn tile(&self, kind: TileKind) -> Handle<Image> {
        match kind {
            TileKind::Forest => self.forest_tile.clone(),
            TileKind::Road => self.road_tile.clone(),
            TileKind::Meadow | TileKind::Hearth => self.meadow_tile.clone(),
        }
    }
}

impl FromWorld for Icons {
    fn from_world(world: &mut World) -> Self {
        let mut images = world.resource_mut::<Assets<Image>>();
        Self {
            chop: images.add(pixel_art(TREE)),
            build: images.add(pixel_art(TOWER)),
            walk: images.add(pixel_art(BOOT)),
            catapult: images.add(pixel_art(CATAPULT)),
            repair: images.add(pixel_art(REPAIR)),
            wood: images.add(pixel_art(LOG)),
            bell: images.add(pixel_art(BELL)),
            hourglass: images.add(pixel_art(HOURGLASS)),
            slime: images.add(pixel_art(SLIME)),
            coin: images.add(pixel_art(COIN)),
            hearth: images.add(pixel_art(HEARTH)),
            forest_tile: images.add(pixel_art(FOREST_TILE)),
            meadow_tile: images.add(pixel_art(MEADOW_TILE)),
            road_tile: images.add(pixel_art(ROAD_TILE)),
        }
    }
}

fn pixel_color(key: char) -> Option<Color> {
    Some(match key {
        'o' => palette::UI_INK,
        'g' => palette::PINE_TIP,
        'G' => palette::PINE,
        'b' => palette::BARK,
        'B' => crate::voxel::shade(palette::BARK, 0.7),
        'e' => palette::LOG_END,
        'E' => crate::voxel::shade(palette::LOG_END, 0.75),
        'r' => palette::ROOF_TOP,
        'R' => palette::ROOF,
        'w' => palette::PLANK,
        'W' => palette::BARK,
        's' => palette::HEARTH_STONE,
        'f' => palette::FLAG,
        'n' => palette::COIN,
        'N' => crate::voxel::shade(palette::BRASS, 0.85),
        'y' => palette::BRASS,
        'Y' => crate::voxel::shade(palette::BRASS, 0.72),
        'c' => palette::UI_PAPER,
        'd' => palette::ROAD,
        'm' => palette::SLIME,
        'M' => crate::voxel::shade(palette::SLIME, 0.7),
        'k' => palette::KEEPER_EYES,
        'l' => palette::UI_PAPER_DARK,
        'S' => crate::voxel::shade(palette::HEARTH_STONE, 0.75),
        'F' => palette::FLAME_OUTER,
        'C' => palette::FLAME_CORE,
        'a' => palette::MEADOW_GRASS,
        'A' => crate::voxel::shade(palette::MEADOW_GRASS, 0.85),
        'h' => palette::FOREST_GRASS,
        'p' => palette::FLOWERS[2],
        'v' => palette::FLOWERS[1],
        'u' => palette::DIRT,
        'U' => palette::DIRT_DARK,
        'D' => crate::voxel::shade(palette::ROAD, 0.85),
        'i' => palette::TOOL_METAL,
        'H' => palette::TOOL_HANDLE,
        _ => return None,
    })
}

fn pixel_art(rows: &[&str]) -> Image {
    let height = rows.len();
    let width = rows[0].len();
    let mut data = Vec::with_capacity(width * height * 4);
    for row in rows {
        assert_eq!(row.len(), width, "icon rows must all be the same width");
        for key in row.chars() {
            let rgba = pixel_color(key).map_or([0; 4], |color| color.to_srgba().to_u8_array());
            data.extend(rgba);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_square_and_use_known_colors() {
        for icon in [
            TREE,
            TOWER,
            CATAPULT,
            REPAIR,
            BOOT,
            LOG,
            BELL,
            HOURGLASS,
            SLIME,
            COIN,
            HEARTH,
            FOREST_TILE,
            MEADOW_TILE,
            ROAD_TILE,
        ] {
            assert_eq!(icon.len(), 12);
            for row in icon {
                assert_eq!(row.len(), 12, "{row:?}");
                for key in row.chars() {
                    assert!(key == '.' || pixel_color(key).is_some(), "unknown {key:?}");
                }
            }
        }
    }
}
