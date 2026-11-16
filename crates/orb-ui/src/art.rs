//! The panel's SVG parts (`assets/ui`), embedded by `build.rs`, and the knob designs.

use eframe::egui;

include!(concat!(env!("OUT_DIR"), "/parts_gen.rs"));

/// A part: its URI for egui's loaders, its SVG bytes and its bleed: how far, in panel pixels,
/// the art reaches beyond its control's rectangle on every side (shadows, halos, glows;
/// task-179). The SVG's canvas is the rectangle grown by the bleed.
#[derive(Clone, Copy)]
pub struct Part {
    pub name: &'static str,
    pub uri: &'static str,
    pub svg: &'static [u8],
    pub bleed: f32,
    /// The control's rectangle (panel pixels) the part is drawn for.
    pub w: f32,
    pub h: f32,
}

impl Part {
    fn from_row(row: &PartRow) -> Self {
        let &(name, uri, svg, bleed, w, h) = row;
        Self {
            name,
            uri,
            svg,
            bleed,
            w,
            h,
        }
    }
}

/// The part called `name` (`PARTS` is sorted by name).
pub fn part(name: &str) -> Option<Part> {
    let i = PARTS.binary_search_by(|p| p.0.cmp(name)).ok()?;
    Some(Part::from_row(&PARTS[i]))
}

/// A photographic image that stands in for the part `name`, decoded once and kept for the
/// process. Without a skin folder it is the built-in skin's (`assets/skin`, embedded by
/// `build.rs`). A skin folder (the one `ORB_UI_SKIN_DIR` names, else [`default_skin_dir`])
/// replaces the built-in skin as a whole: its `<name>.png` is used, and a part it lacks is
/// drawn from its SVG, so an empty folder gives the drawn panel. `None` also when the image
/// does not decode (said once on stderr). The part keeps its rectangle, so a skin moves
/// nothing.
pub fn skin_image(name: &str) -> Option<std::sync::Arc<egui::ColorImage>> {
    type Cache = std::collections::HashMap<String, Option<std::sync::Arc<egui::ColorImage>>>;
    static DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    static IMAGES: std::sync::Mutex<Option<Cache>> = std::sync::Mutex::new(None);
    let dir = DIR.get_or_init(|| {
        std::env::var_os("ORB_UI_SKIN_DIR")
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(default_skin_dir)
    });
    let mut images = IMAGES.lock().ok()?;
    let images = images.get_or_insert_with(Cache::new);
    if let Some(known) = images.get(name) {
        return known.clone();
    }
    let loaded = match dir {
        Some(dir) => {
            let path = dir.join(format!("{name}.png"));
            std::fs::read(&path)
                .ok()
                .and_then(|bytes| decode_photo(&bytes, &path.display().to_string()))
        }
        None => built_in_photo(name).and_then(|bytes| decode_photo(bytes, name)),
    };
    images.insert(name.to_string(), loaded.clone());
    loaded
}

/// The built-in skin's PNG of the part `name` (`SKIN` is sorted by name).
fn built_in_photo(name: &str) -> Option<&'static [u8]> {
    let i = SKIN.binary_search_by(|p| p.0.cmp(name)).ok()?;
    Some(SKIN[i].1)
}

/// The longest side a skin photograph is kept at: the smallest texture limit egui assumes
/// (its default `max_texture_side`), over three times the panel's width.
const PHOTO_MAX_SIDE: u32 = 2048;

/// The decoded PNG `bytes`, scaled down to [`PHOTO_MAX_SIDE`] if larger; a failure is said on
/// stderr with `what` and gives `None`.
fn decode_photo(bytes: &[u8], what: &str) -> Option<std::sync::Arc<egui::ColorImage>> {
    match image::load_from_memory(bytes) {
        Ok(decoded) => {
            let decoded = if decoded.width().max(decoded.height()) > PHOTO_MAX_SIDE {
                decoded.resize(
                    PHOTO_MAX_SIDE,
                    PHOTO_MAX_SIDE,
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                decoded
            };
            let rgba = decoded.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            Some(std::sync::Arc::new(
                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
            ))
        }
        Err(e) => {
            eprintln!("orb: skin: {what}: {e}");
            None
        }
    }
}

/// The skin folder used when `ORB_UI_SKIN_DIR` is not set, if it exists: `skin` in the
/// settings folder (`~/Library/Application Support/OpenRB-338` on macOS, `%APPDATA%\\OpenRB-338`
/// on Windows, `$XDG_CONFIG_HOME/OpenRB-338` or `~/.config/OpenRB-338` elsewhere). A plugin
/// has no environment of its own in a host, so the folder is how it finds a skin.
fn default_skin_dir() -> Option<std::path::PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
    };
    let base = if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else if cfg!(windows) {
        var("APPDATA")?
    } else {
        var("XDG_CONFIG_HOME").or_else(|| Some(var("HOME")?.join(".config")))?
    };
    let dir = base.join("OpenRB-338").join("skin");
    dir.is_dir().then_some(dir)
}

/// The part names asked for that no art has, in this process.
static MISSING: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Records that the part `name` is missing; true the first time it is.
pub fn report_missing(name: &str) -> bool {
    let mut m = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    m.insert(name.to_owned())
}

/// The part names drawn as nothing because no art has them, in this process (task-156).
pub fn missing() -> Vec<String> {
    let m = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    m.iter().cloned().collect()
}

/// Every part.
pub fn all() -> impl Iterator<Item = Part> {
    PARTS.iter().map(Part::from_row)
}

/// A knob design: the layers are `<name>_body` (static) and `<name>_pointer`, rotated about
/// `(cx, cy)` (assets/ui/README.md). The knobs have no value arc since task-179.
pub struct KnobDesign {
    pub name: &'static str,
    pub w: f32,
    pub h: f32,
    pub cx: f32,
    pub cy: f32,
}

pub const KNOBS: [KnobDesign; 5] = [
    KnobDesign {
        name: "knob_303",
        w: 30.0,
        h: 30.0,
        cx: 15.0,
        cy: 15.0,
    },
    KnobDesign {
        name: "knob_808_red",
        w: 13.0,
        h: 13.0,
        cx: 6.5,
        cy: 6.5,
    },
    KnobDesign {
        name: "knob_808_white",
        w: 13.0,
        h: 13.0,
        cx: 6.5,
        cy: 6.5,
    },
    KnobDesign {
        name: "knob_909",
        w: 12.0,
        h: 12.0,
        cx: 6.0,
        cy: 6.0,
    },
    KnobDesign {
        name: "knob_gray",
        w: 18.0,
        h: 19.0,
        cx: 9.0,
        cy: 9.5,
    },
];

/// The design of a knob control, by its layout part name.
pub fn knob_design(part: &str) -> &'static KnobDesign {
    KNOBS
        .iter()
        .find(|k| k.name == part)
        .unwrap_or_else(|| panic!("no knob design {part}"))
}

/// The window's icon: the `app_icon` part rendered at `size` x `size` pixels.
pub fn app_icon(size: u32) -> eframe::egui::IconData {
    let svg = part("app_icon").expect("app_icon part").svg;
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .expect("app_icon parses");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // IconData is straight (not premultiplied) RGBA.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    eframe::egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knob_designs_match_the_art_generator() {
        assert_eq!(KNOBS.len(), orb_ui_art::parts::KNOBS.len());
        for (a, b) in KNOBS.iter().zip(orb_ui_art::parts::KNOBS.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!((a.w, a.h, a.cx, a.cy), (b.w, b.h, b.cx, b.cy));
            // the pointer turns about the canvas's center: the art centers the knob
            assert_eq!((a.cx * 2.0, a.cy * 2.0), (a.w, a.h), "{}", a.name);
        }
    }

    #[test]
    fn all_generated_parts_are_embedded_and_sorted() {
        let generated = orb_ui_art::parts::all_parts();
        assert_eq!(PARTS.len(), generated.len());
        assert!(PARTS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(part("btn_play_lit").is_some());
        assert!(part("panel").is_none());
        assert!(part("nope").is_none());
        // the bleed read from each file is the generator's
        for g in generated {
            let p = part(&g.name).unwrap();
            assert_eq!(p.bleed, g.bleed, "{}", g.name);
            assert!(
                (p.w - g.w).abs() < 1e-4 && (p.h - g.h).abs() < 1e-4,
                "{}",
                g.name
            );
        }
    }

    #[test]
    fn the_built_in_skin_names_parts_and_decodes() {
        assert!(!SKIN.is_empty());
        assert!(SKIN.windows(2).all(|w| w[0].0 < w[1].0));
        for &(name, bytes) in SKIN {
            assert!(part(name).is_some(), "{name} is not a part");
            let photo = decode_photo(bytes, name).unwrap_or_else(|| panic!("{name}"));
            let side = photo.size[0].max(photo.size[1]);
            assert!(side > 0 && side <= PHOTO_MAX_SIDE as usize, "{name}");
        }
        assert!(built_in_photo("bg_tb303").is_some());
        assert!(built_in_photo("nope").is_none());
    }
}
