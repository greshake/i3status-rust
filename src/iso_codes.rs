pub fn lang_to_code(lang: &str) -> Option<&str> {
    match lang {
        // US
        "English" => Some("US"),
        "Cherokee" => Some("US"),
        "Hawaiian" => Some("US"),
        "Serbo-Croatian" => Some("US"),
        // AF
        "Afghani" => Some("AF"),
        "Pashto" => Some("AF"),
        // AR
        "Arabic" => Some("AR"),
        // AL
        "Albanian" => Some("AL"),
        // AM
        "Armenian" => Some("AM"),
        // AZ
        "Azerbaijani" => Some("AZ"),
        // BY
        "Belarusian" => Some("BY"),
        // BE
        "Belgian" => Some("BE"),
        // BD
        "Bangla" => Some("BD"),
        // IN
        "Indian" => Some("IN"),
        "Manipuri" => Some("IN"),
        "Gujarati" => Some("IN"),
        "Punjabi" => Some("IN"),
        "Kannada" => Some("IN"),
        "Malayalam" => Some("IN"),
        "Oriya" => Some("IN"),
        "Ol Chiki" => Some("IN"),
        //"Tamil" => Some("IN"), // TODO: maybe it's `lk`?
        "Telugu" => Some("IN"),
        //"Urdu" => Some("IN"), // TODO: maybe it's `pk`?
        "Hindi" => Some("IN"),
        "Sanskrit" => Some("IN"),
        "Marathi" => Some("IN"),
        "Indic" => Some("IN"),
        // BA
        "Bosnian" => Some("BA"),
        // BG
        "Bulgarian" => Some("BG"),
        // DZ
        "Kabylian" => Some("DZ"),
        // MA
        "Berber" => Some("MA"),
        // CM
        "Cameroon" => Some("CM"),
        "Cameroon Multilingual" => Some("CM"),
        "Mmuock" => Some("CM"),
        // MM
        "Burmese" => Some("MM"),
        "Burmese Zawgyi" => Some("MM"),
        // CA
        "Canadian" => Some("CA"),
        "Inuktitut" => Some("CA"),
        // CN
        "Chinese" => Some("CN"),
        "Tibetan" => Some("CN"),
        "Uyghur" => Some("CN"),
        "Hanyu Pinyin" => Some("CN"),
        // HR
        "Croatian" => Some("HR"),
        // CZ
        "Czech" => Some("CZ"),
        // DK
        "Danish" => Some("DK"),
        // NL
        "Dutch" => Some("NL"),
        // BT
        "Dzongkha" => Some("BT"),
        // EE
        "Estonian" => Some("EE"),
        // IR
        "Persian" => Some("IR"), // Skip Kurdish becuse it also applies to `iq`,  and `tr`
        // IQ
        "Iraqi" => Some("IQ"),
        // FO
        "Faroese" => Some("FO"),
        // FI
        "Finnish" => Some("FI"),
        // FR
        "French" => Some("FR"),
        "Occitan" => Some("FR"),
        // GH
        "Akan" => Some("GH"),
        "Ewe" => Some("GH"),
        "Fula" => Some("GH"),
        "Ga" => Some("GH"),
        "Avatime" => Some("GH"),
        // GE
        "Georgian" => Some("GE"),
        // DE
        "German" => Some("DE"),
        "Lower Sorbian" => Some("DE"),
        // GR
        "Greek" => Some("GR"),
        // HU
        "Hungarian" => Some("HU"),
        // IS
        "Icelandic" => Some("IS"),
        // IL
        "Hebrew" => Some("IL"),
        // IT
        "Italian" => Some("IT"),
        "Sicilian" => Some("IT"),
        "Friulian" => Some("IT"),
        // JP
        "Japanese" => Some("JP"),
        // KG
        "Kyrgyz" => Some("KG"),
        // KH
        "Khmer" => Some("KH"),
        // KZ
        "Kazakh" => Some("KZ"),
        // LA
        "Lao" => Some("LA"),
        // LT
        "Lithuanian" => Some("LT"),
        "Samogitian" => Some("LT"),
        // LV
        "Latvian" => Some("LV"),
        // TODO: Maori
        // ME
        "Montenegrin" => Some("ME"),
        // MK
        "Macedonian" => Some("MK"),
        // MT
        "Maltese" => Some("MT"),
        // MN
        "Mongolian" => Some("MN"),
        // NO
        "Norwegian" => Some("NO"), // Skip Northern Saami
        // PL
        "Polish" => Some("PL"),
        "Kashubian" => Some("PL"),
        "Silesian" => Some("PL"),
        // PT
        "Portuguese" => Some("PT"),
        // RO
        "Romanian" => Some("RO"),
        // RU
        "Russian" => Some("RU"),
        "Tatar" => Some("RU"),
        "Ossetian" => Some("RU"),
        "Chuvash" => Some("RU"),
        "Udmurt" => Some("RU"),
        "Komi" => Some("RU"),
        "Yakut" => Some("RU"),
        "Kalmyk" => Some("RU"),
        "Bashkirian" => Some("RU"),
        "Mari" => Some("RU"),
        "Church Slavonic" => Some("RU"),
        // RS
        "Serbian" => Some("RS"),
        "Pannonian Rusyn" => Some("RS"),
        // SI
        "Slovenian" => Some("SI"),
        // SK
        "Slovak" => Some("SK"),
        // ES
        "Spanish" => Some("ES"),
        "Asturian" => Some("ES"),
        "Catalan" => Some("ES"),
        // SE
        "Swedish" => Some("SE"), // Northern Saami?
        // SY
        "Syriac" => Some("SY"),
        // TJ
        "Tajik" => Some("TJ"),
        // LK
        "Sinhala" => Some("LK"),
        // TH
        "Thai" => Some("TH"),
        // TR
        "Turkish" => Some("TR"),
        "Crimean Tatar" => Some("TR"),
        // TW
        "Taiwanese" => Some("TW"),
        "Saisiyat" => Some("TW"),
        // UA
        "Ukrainian" => Some("UA"),
        // UZ
        "Uzbek" => Some("UZ"),
        // VN
        "Vietnamese" => Some("VN"),
        // KR
        "Korean" => Some("KR"),
        // IE
        "Irish" => Some("IE"),
        "CloGaelach" => Some("IE"),
        "Ogham" => Some("IE"),
        // PK
        "Sindhi" => Some("PK"), // Urdu?
        // MV
        "Dhivehi" => Some("MV"),
        // TODO: Esperanto
        // NP
        "Nepali" => Some("NP"),
        // NG
        "Igbo" => Some("NG"),
        "Yoruba" => Some("NG"),
        "Hausa" => Some("NG"), // Maybe GH?
        // ET
        "Amharic" => Some("ET"),
        // SN
        "Wolof" => Some("SN"),
        // TODO: Braille
        // TM
        "Turkmen" => Some("TM"),
        // ML
        "Bambara" => Some("ML"),
        // TZ
        "Swahili" => Some("TZ"), // Maybe KE?
        // KE
        "Kikuyu" => Some("KE"),
        // BW
        "Tswana" => Some("BW"),
        // PH
        "Filipino" => Some("PH"),
        // MD
        "Moldavian" => Some("MD"),
        // ID
        "Indonesian" => Some("ID"),
        // MY
        "Malay" => Some("MY"),

        // Not found
        _ => None,
    }
}
