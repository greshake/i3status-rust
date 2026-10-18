//! Build the "i3status Icons" TTF and its icon set from the SVG sources in
//! `fonts/i3status-icons/svg`.
//!
//! The output contract must stay stable: 1000 units per em, the 24x24 viewBox
//! mapped to y = -150..850, and glyphs at Private Use Area codepoints from
//! U+E900 assigned in sorted-filename order. Those codepoints are baked into
//! `files/icons/i3status-icons.toml` and into any `icons_overrides` a user has
//! written by codepoint, so reordering them silently changes what people see.
//! Adding a new icon must therefore append a filename that sorts last, or the
//! assignments shift underneath existing configurations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use kurbo::{BezPath, CubicBez, PathEl, Point};
use write_fonts::{
    FontBuilder,
    tables::{
        cmap::Cmap,
        glyf::{GlyfLocaBuilder, Glyph, SimpleGlyph},
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        maxp::Maxp,
        name::{Name, NameRecord},
        os2::Os2,
        post::Post,
    },
    types::{FWord, GlyphId, NameId, Tag, UfWord},
};

const UPM: u16 = 1000;
const ASCENT: i16 = 850;
const DESCENT: i16 = -150;
const VIEWBOX: f64 = 24.0;
const FIRST_CP: u32 = 0xE900;
const FAMILY: &str = "i3status Icons";
const NOTDEF_ADVANCE: u16 = 600;
/// Maximum error when converting cubics to the quadratics `glyf` requires,
/// in font units.
const CU2QU_ACCURACY: f64 = 1.0;

pub fn build_font() -> Result<()> {
    let root = repo_root()?;
    let font_dir = root.join("fonts").join("i3status-icons");
    let svg_dir = font_dir.join("svg");

    // Read the canonical names from the crate itself rather than parsing
    // icons.rs, so the set cannot drift from the code it has to match.
    let canonical: BTreeSet<String> = i3status_rs::icons::Icons::default()
        .0
        .keys()
        .cloned()
        .collect();
    println!("canonical names: {}", canonical.len());

    // Sorted filename order IS the codepoint contract. Do not reorder.
    let mut svg_files: Vec<PathBuf> = std::fs::read_dir(&svg_dir)
        .with_context(|| format!("reading {}", svg_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
        .collect();
    svg_files.sort();
    if svg_files.is_empty() {
        bail!("no SVG sources in {}", svg_dir.display());
    }
    let stems: Vec<String> = svg_files
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    println!("svg sources: {}", stems.len());

    let cp_of: BTreeMap<String, u32> = stems
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), FIRST_CP + i as u32))
        .collect();
    println!(
        "codepoints U+{:04X}..U+{:04X}",
        FIRST_CP,
        FIRST_CP + stems.len() as u32 - 1
    );

    // `name_<digits>` files form an ordered progression; everything else is a
    // single glyph. This is what keeps `bat_0`..`bat_4` together while leaving
    // `bat_charging` alone.
    let mut progressions: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    let mut singles: BTreeSet<String> = BTreeSet::new();
    for stem in &stems {
        match split_progression(stem) {
            Some((base, idx)) => progressions
                .entry(base)
                .or_default()
                .push((idx, stem.clone())),
            None => {
                singles.insert(stem.clone());
            }
        }
    }
    for steps in progressions.values_mut() {
        steps.sort();
    }

    let covered: BTreeSet<&str> = singles
        .iter()
        .map(String::as_str)
        .chain(progressions.keys().map(String::as_str))
        .collect();
    let missing: Vec<&str> = canonical
        .iter()
        .map(String::as_str)
        .filter(|n| !covered.contains(n))
        .collect();
    let extra: Vec<&str> = covered
        .iter()
        .copied()
        .filter(|n| !canonical.contains(*n))
        .collect();
    if !missing.is_empty() {
        bail!("icon names with no SVG source: {missing:?}");
    }
    if !extra.is_empty() {
        bail!("SVG sources that are not icon names: {extra:?}");
    }

    let scale = UPM as f64 / VIEWBOX;
    let mut outlines: BTreeMap<String, BezPath> = BTreeMap::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    for (file, stem) in svg_files.iter().zip(&stems) {
        match svg_to_outline(file, scale) {
            Ok(path) => {
                outlines.insert(stem.clone(), path);
            }
            Err(e) => failed.push((stem.clone(), e.to_string())),
        }
    }
    // Never ship a partial font: a missing glyph would silently fall back to
    // whatever other font happens to cover the codepoint.
    if !failed.is_empty() {
        for (stem, err) in &failed {
            eprintln!("  {stem}: {err}");
        }
        bail!("{} SVG source(s) failed to convert", failed.len());
    }

    let ttf = assemble(&stems, &cp_of, &outlines)?;
    let ttf_path = font_dir.join("i3status-icons.ttf");
    std::fs::write(&ttf_path, &ttf).with_context(|| format!("writing {}", ttf_path.display()))?;
    println!("wrote {} ({} bytes)", ttf_path.display(), ttf.len());

    let toml = render_icon_set(&canonical, &progressions, &cp_of);
    let toml_path = root.join("files").join("icons").join("i3status-icons.toml");
    std::fs::write(&toml_path, toml).with_context(|| format!("writing {}", toml_path.display()))?;
    println!("wrote {}", toml_path.display());

    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask must live one level below the repository root")?
        .to_path_buf())
}

/// `bat_3` -> `("bat", 3)`. The suffix must be all digits, which is what keeps
/// `bat_charging` and `volume_muted` out of the progressions.
fn split_progression(stem: &str) -> Option<(String, u32)> {
    let (base, idx) = stem.rsplit_once('_')?;
    if idx.is_empty() || !idx.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((base.to_string(), idx.parse().ok()?))
}

/// Parse one SVG, turn every stroke into a filled outline, and map the 24x24
/// viewBox into font units with the y axis flipped (SVG y-down -> font y-up).
fn svg_to_outline(file: &Path, scale: f64) -> Result<BezPath> {
    let data = std::fs::read(file)?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default())?;
    let mut out = BezPath::new();
    collect(tree.root(), &mut out, scale)?;
    if out.elements().is_empty() {
        bail!("produced no contours");
    }
    // `glyf` stores quadratics only; SimpleGlyph::from_bezpath rejects cubics.
    Ok(cubics_to_quads(&out))
}

fn collect(group: &usvg::Group, out: &mut BezPath, scale: f64) -> Result<()> {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect(g, out, scale)?,
            usvg::Node::Path(p) => {
                // SVG applies the stroke in the element's own user space and
                // transforms the result, so stroke first and transform after.
                // Several sources put translate+scale on a group; ignoring it
                // moves the ink by as much as a fifth of the em.
                let transform = p.abs_transform();
                if let Some(stroke) = p.stroke() {
                    let outline = p
                        .data()
                        .clone()
                        .stroke(&to_stroke(stroke), 1.0)
                        .context("stroke-to-path failed")?
                        .transform(transform)
                        .context("transforming stroke outline failed")?;
                    append(&outline, out, scale);
                }
                if p.fill().is_some() {
                    let filled = p
                        .data()
                        .clone()
                        .transform(transform)
                        .context("transforming fill failed")?;
                    append(&filled, out, scale);
                }
            }
            usvg::Node::Text(_) => bail!("SVG contains text; convert it to outlines first"),
            usvg::Node::Image(_) => bail!("SVG contains a raster image"),
        }
    }
    Ok(())
}

fn to_stroke(s: &usvg::Stroke) -> tiny_skia_path::Stroke {
    tiny_skia_path::Stroke {
        width: s.width().get(),
        miter_limit: s.miterlimit().get(),
        line_cap: match s.linecap() {
            usvg::LineCap::Butt => tiny_skia_path::LineCap::Butt,
            usvg::LineCap::Round => tiny_skia_path::LineCap::Round,
            usvg::LineCap::Square => tiny_skia_path::LineCap::Square,
        },
        line_join: match s.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => tiny_skia_path::LineJoin::Miter,
            usvg::LineJoin::Round => tiny_skia_path::LineJoin::Round,
            usvg::LineJoin::Bevel => tiny_skia_path::LineJoin::Bevel,
        },
        ..Default::default()
    }
}

/// Append a tiny-skia path to a kurbo path, applying `x * scale` and the y-flip
/// `ASCENT - y * scale` in the same pass.
fn append(path: &tiny_skia_path::Path, out: &mut BezPath, scale: f64) {
    let pt = |x: f32, y: f32| Point::new(x as f64 * scale, ASCENT as f64 - y as f64 * scale);
    for seg in path.segments() {
        match seg {
            tiny_skia_path::PathSegment::MoveTo(p) => out.move_to(pt(p.x, p.y)),
            tiny_skia_path::PathSegment::LineTo(p) => out.line_to(pt(p.x, p.y)),
            tiny_skia_path::PathSegment::QuadTo(a, b) => out.quad_to(pt(a.x, a.y), pt(b.x, b.y)),
            tiny_skia_path::PathSegment::CubicTo(a, b, c) => {
                out.curve_to(pt(a.x, a.y), pt(b.x, b.y), pt(c.x, c.y))
            }
            tiny_skia_path::PathSegment::Close => out.close_path(),
        }
    }
}

fn cubics_to_quads(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut cursor = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                out.move_to(p);
                cursor = p;
            }
            PathEl::LineTo(p) => {
                out.line_to(p);
                cursor = p;
            }
            PathEl::QuadTo(a, b) => {
                out.quad_to(a, b);
                cursor = b;
            }
            PathEl::CurveTo(a, b, c) => {
                for (_, _, q) in CubicBez::new(cursor, a, b, c).to_quads(CU2QU_ACCURACY) {
                    out.quad_to(q.p1, q.p2);
                }
                cursor = c;
            }
            PathEl::ClosePath => out.close_path(),
        }
    }
    out
}

fn notdef() -> Result<SimpleGlyph> {
    let mut p = BezPath::new();
    p.move_to((100.0, 0.0));
    p.line_to((100.0, 700.0));
    p.line_to((500.0, 700.0));
    p.line_to((500.0, 0.0));
    p.close_path();
    SimpleGlyph::from_bezpath(&p).map_err(|e| anyhow::anyhow!("building .notdef: {e:?}"))
}

fn assemble(
    stems: &[String],
    cp_of: &BTreeMap<String, u32>,
    outlines: &BTreeMap<String, BezPath>,
) -> Result<Vec<u8>> {
    let mut glyphs = vec![notdef()?];
    for stem in stems {
        glyphs.push(
            SimpleGlyph::from_bezpath(&outlines[stem])
                .map_err(|e| anyhow::anyhow!("building glyph {stem}: {e:?}"))?,
        );
    }

    let (mut x_min, mut y_min) = (i16::MAX, i16::MAX);
    let (mut x_max, mut y_max) = (i16::MIN, i16::MIN);
    let mut metrics = Vec::with_capacity(glyphs.len());
    for (i, g) in glyphs.iter().enumerate() {
        x_min = x_min.min(g.bbox.x_min);
        y_min = y_min.min(g.bbox.y_min);
        x_max = x_max.max(g.bbox.x_max);
        y_max = y_max.max(g.bbox.y_max);
        metrics.push(LongMetric {
            advance: if i == 0 { NOTDEF_ADVANCE } else { UPM },
            side_bearing: g.bbox.x_min,
        });
    }

    let mut builder = GlyfLocaBuilder::new();
    for g in &glyphs {
        builder.add_glyph(&Glyph::Simple(g.clone()))?;
    }
    let (glyf, loca, loca_format) = builder.build();

    let num_glyphs = glyphs.len() as u16;
    let head = Head {
        units_per_em: UPM,
        x_min,
        y_min,
        x_max,
        y_max,
        index_to_loc_format: loca_format as i16,
        ..Default::default()
    };
    let hhea = Hhea {
        ascender: FWord::new(ASCENT),
        descender: FWord::new(DESCENT),
        line_gap: FWord::new(0),
        advance_width_max: UfWord::new(metrics.iter().map(|m| m.advance).max().unwrap_or(UPM)),
        min_left_side_bearing: FWord::new(
            metrics.iter().map(|m| m.side_bearing).min().unwrap_or(0),
        ),
        min_right_side_bearing: FWord::new(0),
        x_max_extent: FWord::new(x_max),
        caret_slope_rise: 1,
        caret_slope_run: 0,
        caret_offset: 0,
        number_of_h_metrics: num_glyphs,
    };
    let os2 = Os2 {
        s_typo_ascender: ASCENT,
        s_typo_descender: DESCENT,
        s_typo_line_gap: 0,
        us_win_ascent: ASCENT as u16,
        us_win_descent: (-DESCENT) as u16,
        ach_vend_id: Tag::new(b"NONE"),
        ..Default::default()
    };

    let mut order = vec![".notdef".to_string()];
    order.extend(stems.iter().map(|s| format!("uni{:04X}", cp_of[s])));

    let cmap = Cmap::from_mappings(stems.iter().enumerate().map(|(i, s)| {
        (
            char::from_u32(cp_of[s]).expect("PUA codepoints are valid scalar values"),
            GlyphId::new(i as u32 + 1),
        )
    }))?;

    // Distributions package fonts separately, which physically separates the
    // TTF from the repository LICENSE, so the file carries its own statement.
    let name = Name::new(
        [
            (
                NameId::COPYRIGHT_NOTICE,
                "Copyright (c) 2026 i3status-rust contributors.",
            ),
            (NameId::FAMILY_NAME, FAMILY),
            (NameId::SUBFAMILY_NAME, "Regular"),
            (NameId::UNIQUE_ID, "i3statusIcons-Regular"),
            (NameId::FULL_NAME, FAMILY),
            (NameId::VERSION_STRING, "Version 1.000"),
            (NameId::POSTSCRIPT_NAME, "i3statusIcons-Regular"),
            (
                NameId::LICENSE_DESCRIPTION,
                "GNU General Public License version 3 only (GPL-3.0-only). \
                 See the LICENSE file of the i3status-rust distribution.",
            ),
            (
                NameId::LICENSE_URL,
                "https://github.com/greshake/i3status-rust/blob/master/LICENSE",
            ),
        ]
        .into_iter()
        .map(|(id, value)| NameRecord::new(3, 1, 0x409, id, value.to_string().into()))
        .collect(),
    );

    let mut fb = FontBuilder::new();
    fb.add_table(&head)?;
    fb.add_table(&hhea)?;
    fb.add_table(&Maxp {
        num_glyphs,
        ..Default::default()
    })?;
    fb.add_table(&os2)?;
    fb.add_table(&Hmtx::new(metrics, Vec::new()))?;
    fb.add_table(&cmap)?;
    fb.add_table(&name)?;
    fb.add_table(&Post::new_v2(order.iter().map(String::as_str)))?;
    fb.add_table(&glyf)?;
    fb.add_table(&loca)?;
    Ok(fb.build())
}

fn render_icon_set(
    canonical: &BTreeSet<String>,
    progressions: &BTreeMap<String, Vec<(u32, String)>>,
    cp_of: &BTreeMap<String, u32>,
) -> String {
    let escape = |stem: &str| format!("\\u{:04x}", cp_of[stem]);
    let mut out = format!(
        "# i3status-rust icon set for the \"{FAMILY}\" font.\n\
         # Generated by `cargo xtask build-font` -- do not edit by hand.\n\
         # Codepoints run from U+{FIRST_CP:04X} in sorted SVG filename order.\n\n"
    );
    // Bare keys, matching the other icon sets: verify_icon_files.sh compares
    // the first whitespace-separated field against icons.rs, so quoted keys
    // would make every name look like a conflict.
    for name in canonical {
        if let Some(steps) = progressions.get(name) {
            let values: Vec<String> = steps
                .iter()
                .map(|(_, stem)| format!("\"{}\"", escape(stem)))
                .collect();
            out.push_str(&format!("{name} = [{}]\n", values.join(", ")));
        } else {
            out.push_str(&format!("{name} = \"{}\"\n", escape(name)));
        }
    }
    out
}
