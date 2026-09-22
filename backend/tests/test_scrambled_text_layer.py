from quantix.documents import scrambled_text_layer


def test_broken_font_mapping_is_detected():
    scrambled = (
        "ʹͳ\n\x0b\x16\x0c\x03ΝΫϭϣϧ\n\x03ΔΑϭϠρϣϟ\u038d\x03ΕΎΟϭϠΗϛϟ\u038d\x03ϭ\u038d\x03ΕΎϧϳόϠϟ\x03Δϳϧϔϟ\u038d"
        "\n6SHFLILFDWLRQV\x03DQG\x03WHFKQLFDO\x03LQIRUPDWLRQ\x03IRU\x03WKH\x03VDPSOHV\x03UHTXLUHG"
    )
    assert scrambled_text_layer(scrambled) is True


def test_real_english_and_arabic_text_is_kept():
    english = "Specifications and technical information for the samples required. " * 3
    arabic = (
        "إنشاء وبناء محطة خدمات مكافحة الحريق في جامعة الملك سعود بن عبدالعزيز للعلوم الصحية " * 2
    )
    assert scrambled_text_layer(english) is False
    assert scrambled_text_layer(arabic) is False
    assert scrambled_text_layer("Page 3") is False
    assert scrambled_text_layer("Pump head ΔP = 5 kPa, 220 Ω") is False


def test_short_scrambled_title_page_is_detected():
    assert scrambled_text_layer("ͳ\nΔϣΎόϟ\u038d ΔϴϨϔϟ\u038d ρϭήθ˰ϟ\u038d") is True
    assert scrambled_text_layer("ͷͶ\nΕΎϳϣϛϟ\u038d\x03ϝϭΩΟ") is True
