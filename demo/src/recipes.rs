//! The 100 demos. Demo 0 is the original UFO demo; the other 99 are generated
//! deterministically from the effect library: each gets a title, a (fictional)
//! group, two palettes, 5–7 parts around a theme, a transition style and its own
//! scroll text.

use crate::fx::{self, Def, Rng};

pub const COUNT: usize = 100;

pub struct Recipe {
    pub name: &'static str,
    pub group: &'static str,
    /// Palette indices: parts alternate between the two.
    pub pal: [usize; 2],
    /// (index into `fx::all()`, seed) per part.
    pub parts: Vec<(usize, u32)>,
    /// 0 flash + blinds, 1 fade, 2 dissolve, 3 wipe, 4 mosaic.
    pub transition: u8,
    /// Logo over parts that allow it: 0 chrome logo on top, 1 small floating logo.
    pub logo: u8,
    pub text: String,
}

#[rustfmt::skip]
const NAMES: [&str; COUNT] = [
    "UFO", "NORRSKEN", "KOMETEN", "MIDVINTER", "FROSTBYTE", "NEONÄLGEN", "STJÄRNSTOFT", "SOLVARV",
    "BLIXTLÅS", "POLARNATT", "ISKRISTALL", "MÅNSKEN", "VÅRFLOD", "HÖSTMÖRKER", "SJÖSTJÄRNA", "FYRTORNET",
    "GLASPÄRLOR", "SNÖSTORM", "SKÄRGÅRD", "MIDSOMMAR", "KANELBULLE", "LAKRITS", "SALMIAK", "TUNNELBANAN",
    "RYMDRESAN", "ROBOTDANS", "KVASAR", "PULSAR", "NEBULOSA", "MAGNETAR", "SUPERNOVA", "HYPERRYMD",
    "TIDSMASKINEN", "SPEGELSALEN", "KROMOSFÄR", "PLASMAKUNGEN", "VEKTORVALS", "PIXELPOLKA", "BITFLIP",
    "KOPPARTOPP", "RASTERRUS", "SPRITEFEST", "DISKETTEN", "KASSETTBANDET", "JOYSTICKEN", "SCANLINE",
    "INTERLACE", "HALFBRITE", "BITPLANET", "CHIPMINNET", "NOLLÅTTA", "VINTERVILA", "ÅSKVÄDER", "REGNBÅGEN",
    "KVICKSILVER", "GULDFEBER", "ROSTFRI", "TROLLSKOGEN", "MYRSTACKEN", "ÄLVDANS", "SJÖODJURET",
    "DRAKBÅTEN", "RUNSTENEN", "BASTUN", "TÄRNINGEN", "KUBEN", "KLOTET", "SPIRALEN", "VIRVELN",
    "ÖGONBLICK", "HJÄRTSLAG", "TAKTPINNEN", "BASGÅNG", "TRUMVIRVEL", "SYNTVÅG", "ANALOGT", "MODULÄR",
    "OSCILLATOR", "FREKVENS", "AMPLITUD", "RESONANS", "EKOLOD", "RADARSKÄRM", "SATELLIT", "MÅNLANDNING",
    "TYNGDLÖS", "SVART HÅL", "HORISONTEN", "STJÄRNPORTEN", "VINTERGATAN", "ANDROMEDA", "KARLAVAGNEN",
    "POLSTJÄRNAN", "NATTÅGET", "GRYNING", "SKYMNING", "MORGONROCK", "SOMMARREGN", "FJÄRILSEFFEKT",
    "SISTA DANSEN",
];

#[rustfmt::skip]
const GROUPS: [&str; 12] = [
    "NORDLJUS", "BITVARGARNA", "KOPPARGRUVAN", "PIXELPATRULLEN", "BLITTERBRÖDERNA", "TAKTFASTA",
    "KRETSKORTET", "NOLLOR OCH ETTOR", "MODEMKLUBBEN", "SPRITESMEDJAN", "VEKTORVERKSTAN", "UFO",
];

const ORIGINAL_TEXT: &str = "HEJ OCH VÄLKOMMEN!   UFO IS BACK WITH A BRAND NEW DEMO: A HYMN TO THE SWEDISH DEMO SCENE ...   \
EVERY PIXEL IS PUSHED BY RUST CODE COMPILED TO WEBASSEMBLY, IN GLORIOUS 12-BIT COLOUR ...   \
THE COPPER BARS, CUBES AND BALLS ARE DANCING TO WHATEVER SPOTIFY IS PLAYING RIGHT NOW, \
SO TURN IT UP!   ...   KEYS: N = NEXT PART, F = FULLSCREEN, H = INFO   ...   \
GREETINGS TO EVERYONE WHO EVER CARRIED A COMPUTER AND A CRT INTO A COLD PARTY HALL \
IN THE MIDDLE OF WINTER, AND STAYED UP ALL NIGHT TO MAKE SOMETHING MOVE ...   \
UFO IS COOL!   UFO IS THE WINNER!   ...   SEE YOU AT THE NEXT PARTY!   ...   LET'S WRAP ...          ";

#[rustfmt::skip]
const LINES: [&str; 24] = [
    "CODED IN ONE LONG NIGHT WITH TOO MUCH COFFEE AND TOO LITTLE SLEEP.",
    "ALL EFFECTS IN REAL TIME, NO VIDEO, NO TRICKS ... WELL, SOME TRICKS.",
    "TURN UP THE MUSIC, THE PIXELS ARE LISTENING!",
    "THIS PART IS DEDICATED TO EVERYONE STILL OWNING A FLOPPY DISK.",
    "12-BIT COLOUR WAS ENOUGH THEN AND IT IS ENOUGH NOW.",
    "VI SES PÅ NÄSTA PARTY, TA MED FÖRLÄNGNINGSSLADD!",
    "NO RASTER LINES WERE HARMED IN THE MAKING OF THIS DEMO.",
    "THE BEAT DETECTOR SAYS YOUR MUSIC IS EXCELLENT.",
    "KAFFE, KANELBULLAR OCH ASSEMBLER - DET ÄR LIVET.",
    "WHO NEEDS A GPU WHEN YOU HAVE A FRAMEBUFFER AND A DREAM?",
    "PRESS N IF YOU ARE BORED, BUT WE KNOW YOU ARE NOT.",
    "SEND YOUR BEST WISHES TO THE COPPER, IT WORKS SO HARD.",
    "THIS SCROLLER IS LONGER THAN IT NEEDS TO BE, AND WE ARE PROUD OF IT.",
    "HÄLSNINGAR FRÅN EN KALL KÄLLARE NÅGONSTANS I NORRLAND.",
    "EVERY DOT ON THIS SCREEN WAS PLACED WITH LOVE.",
    "IF YOU CAN READ THIS, THE SINE WAVE IS NOT BIG ENOUGH.",
    "MORE BARS! MORE BALLS! MORE EVERYTHING!",
    "THIS DEMO RUNS ON RUST, WEBASSEMBLY AND PURE ENTHUSIASM.",
    "THE PALETTE WAS CHOSEN BY A COMMITTEE OF ONE.",
    "NOW CAST THIS TO THE BIG SCREEN AND CALL THE NEIGHBOURS.",
    "TIDEN STÅR STILL NÄR BASEN GÅR.",
    "REMEMBER: A GOOD DEMO NEVER ENDS, IT JUST LOOPS.",
    "BEST VIEWED ON A CRT AT THREE IN THE MORNING.",
    "NO AI WAS ... OKAY, MAYBE A LITTLE.",
];

/// Boasts about UFO: every demo's scroll text gets two of them.
#[rustfmt::skip]
const UFO_LINES: [&str; 21] = [
    "UFO IS COOL!",
    "UFO IS THE WINNER!",
    "UFO RULES THE SCENE!",
    "UFO WINS THE DEMO COMPO - AGAIN!",
    "FIRST PLACE: UFO. SECOND PLACE: ALSO UFO.",
    "UFO IS FASTER THAN A BLITTER.",
    "UFO NEVER DROPS A FRAME.",
    "UFO: 4096 COLOURS, ZERO EXCUSES.",
    "UFO IS THE KING OF THE RASTER BARS.",
    "UFO BEAT THE BEAT AGAIN.",
    "UFO MAKES YOUR SPEAKERS SMILE.",
    "UFO IS COOLER THAN A FROZEN FLOPPY.",
    "THE UFO HAS LANDED - PARTY TIME!",
    "TRUST THE UFO.",
    "UFO > EVERYTHING ELSE.",
    "NO COPPER WAS HARMED IN THE MAKING OF UFO.",
    "UFO ÄR BÄST!",
    "UFO ÄR COOLAST I HELA GALAXEN.",
    "UFO VANN IGEN!",
    "SVERIGES SNABBASTE TEFAT: UFO.",
    "UFO GÅR ALDRIG OFFLINE.",
];

/// Did-you-know lines about demosceners who went on to fame. Shown seldom: one in about
/// every fourth demo's scroll text.
#[rustfmt::skip]
const FACTS: [&str; 10] = [
    "DID YOU KNOW? DICE, THE STUDIO BEHIND BATTLEFIELD, WAS STARTED IN VÄXJÖ BY MEMBERS OF THE SWEDISH AMIGA GROUP THE SILENTS.",
    "DID YOU KNOW? DREAMHACK, NOW ONE OF THE BIGGEST COMPUTER FESTIVALS IN THE WORLD, BEGAN AS A SMALL PARTY IN MALUNG.",
    "DID YOU KNOW? JESPER KYD OF THE SILENTS WENT FROM AMIGA DEMOS TO THE MUSIC OF HITMAN AND ASSASSIN'S CREED.",
    "DID YOU KNOW? FUTURE CREW MADE SECOND REALITY, AND ITS MEMBERS WENT ON TO FOUND REMEDY AND FUTUREMARK.",
    "DID YOU KNOW? PURPLE MOTION OF FUTURE CREW NOW WRITES MUSIC FOR ORCHESTRAS AND GAMES.",
    "DID YOU KNOW? SKAVEN OF FUTURE CREW MADE THE MUSIC FOR GAMES LIKE BEJEWELED.",
    "DID YOU KNOW? HOUSEMARQUE, THE MAKERS OF RESOGUN AND RETURNAL, WAS FOUNDED BY FINNISH DEMOSCENERS.",
    "DID YOU KNOW? BJORN LYNNE WENT FROM AMIGA MUSIC TO GAME SOUNDTRACKS LIKE WORMS.",
    "DID YOU KNOW? THE LORDS OF MIDNIGHT (1984) BY MIKE SINGLETON SQUEEZED A WHOLE FANTASY WORLD OF MOUNTAINS, FORESTS AND ARMIES INTO A 48K ZX SPECTRUM.",
    "DID YOU KNOW? DESERT RATS (1985) LET YOU REFIGHT THE NORTH AFRICA CAMPAIGN OF THE SECOND WORLD WAR, HEX BY HEX, ON A ZX SPECTRUM.",
];

/// All 100 demos for the effect list `defs` (from `fx::all()`).
pub fn build(defs: &[Def]) -> Vec<Recipe> {
    let by_name = |n: &str| defs.iter().position(|d| d.name == n).expect("classic effect");
    let mut out = vec![Recipe {
        name: NAMES[0],
        group: "UFO",
        pal: [0, 0],
        parts: ["COPPER", "GLENZ", "VECTOR BALLS", "TWISTER", "TUNNEL", "KEFRENS", "SPECTRUM"]
            .iter()
            .enumerate()
            .map(|(i, n)| (by_name(n), i as u32))
            .collect(),
        transition: 0,
        logo: 0,
        text: ORIGINAL_TEXT.to_string(),
    }];

    let pool: Vec<usize> = (0..defs.len()).filter(|&i| defs[i].flags & fx::IMAGE == 0).collect();
    let themes = [fx::PIXEL, fx::DEFORM, fx::VECTOR, fx::RASTER | fx::RETRO8, 0];
    let mut used = vec![0u32; defs.len()];
    for (k, &name) in NAMES.iter().enumerate().skip(1) {
        let mut rng = Rng::new(k as u32 * 7919 + 13);
        let theme = themes[k % themes.len()];
        let n = 5 + rng.below(3) as usize;
        let mut parts: Vec<(usize, u32)> = Vec::new();
        while parts.len() < n {
            // two thirds from the theme, the rest from anywhere
            let in_theme = theme != 0 && rng.below(3) < 2;
            let cands: Vec<usize> = pool
                .iter()
                .copied()
                .filter(|&i| !in_theme || defs[i].flags & theme != 0)
                .filter(|&i| !parts.iter().any(|p| p.0 == i))
                .collect();
            if cands.is_empty() {
                continue;
            }
            // of three random candidates, take the least used one so every effect gets airtime
            let pick = (0..3)
                .map(|_| cands[rng.below(cands.len() as u32) as usize])
                .min_by_key(|&i| used[i])
                .unwrap_or(cands[0]);
            used[pick] += 1;
            parts.push((pick, rng.u32()));
        }
        let group = GROUPS[rng.below(GROUPS.len() as u32) as usize];
        let p0 = rng.below(16) as usize;
        let p1 = (p0 + 1 + rng.below(15) as usize) % 16;
        let boast = rng.below(UFO_LINES.len() as u32) as usize;
        let boast2 = (boast + 1 + rng.below(UFO_LINES.len() as u32 - 1) as usize) % UFO_LINES.len();
        let mut text = format!("{group} PRESENTS {name}, A HYMN TO THE SWEDISH DEMO SCENE ...   {}   ...   ", UFO_LINES[boast]);
        let mut lines: Vec<usize> = (0..LINES.len()).collect();
        for _ in 0..4 {
            let i = rng.below(lines.len() as u32) as usize;
            text += LINES[lines.remove(i)];
            text += "   ...   ";
        }
        let greets: Vec<&str> = GROUPS.iter().copied().filter(|g| *g != group).collect();
        text += UFO_LINES[boast2];
        text += "   ...   GREETINGS TO ";
        text += &greets.join(", ");
        // seldom: a fact about the scene, each one in turn
        if k % 4 == 1 {
            text += "   ...   ";
            text += FACTS[(k / 4) % FACTS.len()];
        }
        // now and then: ask to share, and (more seldom) where the code is
        if k % 3 == 0 {
            text += "   ...   LIKE THIS? SHARE UFO WITH YOUR FRIENDS!";
        }
        if k % 5 == 2 {
            text += "   ...   UFO IS FREE AND OPEN SOURCE: ";
            text += crate::demo::HOME;
        }
        text += "   ...   KEYS: N NEXT PART, PIL VÄNSTER/HÖGER BYT DEMO, F FULLSCREEN, H INFO ...          ";
        out.push(Recipe {
            name,
            group,
            pal: [p0, p1],
            parts,
            transition: rng.below(5) as u8,
            logo: rng.below(2) as u8,
            text,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundred_distinct_demos() {
        let defs = fx::all();
        let r = build(&defs);
        assert_eq!(r.len(), COUNT);
        let mut names = std::collections::HashSet::new();
        let mut combos = std::collections::HashSet::new();
        for d in &r {
            assert!(names.insert(d.name), "duplicate title {}", d.name);
            let mut ids: Vec<usize> = d.parts.iter().map(|p| p.0).collect();
            ids.sort();
            assert!(combos.insert(ids), "{} repeats another demo's parts", d.name);
            assert!(d.parts.len() >= 5);
            for ch in d.name.chars().chain(d.text.chars()) {
                assert!(ch == ' ' || crate::font::glyph(ch) != [0; 7], "{:?} not in the font ({})", ch, d.name);
            }
        }
    }
}
