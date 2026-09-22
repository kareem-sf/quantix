from quantix.documents import (
    _attach_strays,
    _join_mark_breaks,
    pdf_page_text,
    reading_order_line,
)


def glyphs(*words, start=0.0, step=4.0, bottom=0.0, top=8.0):
    """Lay words out on one baseline in the order given, left to right. Each
    word's characters are drawn right to left when it is Arabic, as a PDF
    writer that stores a line in visual order does."""

    line = []
    x = start
    for word in words:
        # A tuple is one word whose parts touch with no space between them.
        for part in (word,) if isinstance(word, str) else word:
            arabic = any("؀" <= character <= "ۿ" for character in part)
            boxes = [(x + index * step, x + (index + 1) * step) for index in range(len(part))]
            if arabic:
                boxes.reverse()
            line.extend(
                (character, left, right, bottom, top)
                for character, (left, right) in zip(part, boxes, strict=True)
            )
            x += len(part) * step
        line.append((" ", x, x + step, bottom, bottom))
        x += step
    return line[:-1]


def test_visual_order_arabic_line_is_read_right_to_left():
    stored = glyphs("1300", "مكعب", "متر")
    assert reading_order_line(stored) == "متر مكعب 1300"


def test_english_words_inside_arabic_line_keep_their_order():
    stored = glyphs("العمل", "PEB", "Structure", "الـ", "واعتماد")
    assert reading_order_line(stored) == "واعتماد الـ PEB Structure العمل"


def test_logical_order_lines_are_returned_as_stored():
    words = [[]]
    for glyph in glyphs("السعودية", "العربية", "المملكة"):
        if glyph[0] == " ":
            words.append([])
        else:
            words[-1].append(glyph)
    # Stored in reading order: the rightmost word first.
    arabic = [*words[2], (" ", 0, 0, 0, 0), *words[1], (" ", 0, 0, 0, 0), *words[0]]
    assert reading_order_line(arabic) == "المملكة العربية السعودية"
    english = glyphs("Pump", "head", "5", "kPa")
    assert reading_order_line(english) == "Pump head 5 kPa"


def test_arabic_glued_to_english_is_split():
    stored = glyphs("Fire", ("Alarm", "ونظام"), "بين", "الربط")
    assert reading_order_line(stored) == "الربط بين ونظام Fire Alarm"


def test_leading_tanween_and_full_stop_move_after_the_word():
    tanween = [("ً", 0.5, 1.5, 9, 10)] + [
        (character, left, left + 3, 0, 8)
        for character, left in zip("ايضا", (12, 9, 3, 0), strict=True)
    ]
    assert reading_order_line(tanween) == "ايضاً"
    stop = [(".", 0, 1, 0, 2)] + [
        (character, left, left + 3, 0, 8)
        for character, left in zip("للحفر", (14, 11, 8, 5, 2), strict=True)
    ]
    assert reading_order_line(stop) == "للحفر."


def test_lam_alef_ligature_stored_alef_first_is_repaired():
    stored = [
        ("ا", 20, 21, 0, 8),
        ("ا", 15, 19, 0, 8),
        ("ل", 15, 19, 0, 8),
        ("س", 10, 15, 0, 8),
    ]
    assert reading_order_line(stored) == "الاس"


def test_lam_ya_ligature_stored_lam_last_is_repaired():
    stored = [("ا", 20, 21, 0, 8), ("ى", 12, 19, 0, 8), ("ل", 12, 19, 0, 8)]
    assert reading_order_line(stored) == "الى"


def test_pieces_pdfium_puts_on_their_own_line_go_back_into_their_word():
    # The dots of a final ya, drawn just under the fa they belong to.
    dots = [("ي", 5, 7, 18, 19)]
    blank = [(" ", 6, 7, 30, 31)]
    word = [
        ("ف", 2, 10, 20, 28),
        (" ", 10, 12, 20, 20),
        *glyphs("الحريق", start=12, bottom=20, top=28),
    ]
    # A letter drawn above its word is left alone: fonts label a dot that way.
    dot_above = [("ز", 5, 7, 29, 30)]
    lines = _attach_strays([glyphs("بن", bottom=40, top=48), dots, blank, word, dot_above])
    texts = ["".join(glyph[0] for glyph in line) for line in lines]
    assert texts[0] == "بن" and "ز" in texts
    assert reading_order_line(lines[2]) == "الحريق في"
    tanween = [("ً", 0.5, 1.5, 9, 10)]
    host = [("ا", 12, 15, 0, 8), ("ي", 9, 12, 0, 8), ("ض", 3, 9, 0, 8), ("ا", 0, 3, 0, 8)]
    alone = _attach_strays([tanween, host])
    assert len(alone) == 1 and reading_order_line(alone[0]) == "ايضاً"


def test_line_break_after_raised_mark_is_joined():
    first = glyphs("عن", "التحري") + [(" ", 50, 52, 0, 0), ("ً", 52, 53, 9, 10)]
    second = [
        (character, left, left + 3, 0, 8)
        for character, left in zip("ايضا", (61, 58, 55, 52), strict=True)
    ]
    other = [("x", 0, 3, 30, 38)]
    joined = _join_mark_breaks([first, second, other])
    assert len(joined) == 2
    assert "ايضاً" in reading_order_line(joined[0])


class FakeTextPage:
    def __init__(self, lines):
        self.stream = []
        for number, line in enumerate(lines):
            if number:
                self.stream.extend([("\r", 0, 0, 0, 0), ("\n", 0, 0, 0, 0)])
            self.stream.extend(line)

    def get_text_bounded(self, errors="replace"):
        return "".join(glyph[0] for glyph in self.stream)

    def count_chars(self):
        return len(self.stream)

    def get_text_range(self, index, count, errors="replace"):
        return "".join(glyph[0] for glyph in self.stream[index : index + count])

    def get_charbox(self, index):
        character, left, right, bottom, top = self.stream[index]
        return left, bottom, right, top


def test_page_text_reorders_arabic_lines_only():
    page = FakeTextPage(
        [
            glyphs("1.1"),
            glyphs("الحالية", "للإعمال", "والترحيل", bottom=20, top=28),
            glyphs("Quantity", "1300", bottom=40, top=48),
        ]
    )
    assert pdf_page_text(page) == "1.1\nوالترحيل للإعمال الحالية\nQuantity 1300"
    english = FakeTextPage([glyphs("Bill", "of", "quantities")])
    assert pdf_page_text(english) == "Bill of quantities"


def test_bracket_kept_with_the_wrong_word_goes_where_it_is_drawn():
    # "(Solid Slab) متر مكعب 45" stored as "45 مكعب متر) Solid Slab) كمرية":
    # the first bracket is stored after "متر" but drawn against "Solid".
    line = [
        ("4", 259, 262, 503, 508),
        ("5", 263, 265, 503, 508),
        (" ", 265, 267, 503, 503),
        *[(c, x, x + 4, 503, 508) for c, x in zip("مكعب", (297, 294, 291, 286), strict=True)],
        (" ", 301, 303, 503, 503),
        *[(c, x, x + 4, 503, 508) for c, x in zip("متر", (308, 306, 303), strict=True)],
        (")", 428, 430, 501, 508),
        (" ", 309, 309, 503, 503),
        *[(c, 431 + 3 * i, 434 + 3 * i, 503, 508) for i, c in enumerate("Solid")],
        (" ", 447, 449, 503, 503),
        *[(c, 449 + 3 * i, 452 + 3 * i, 503, 508) for i, c in enumerate("Slab")],
        (")", 463, 465, 501, 508),
        (" ", 466, 468, 503, 503),
        *[(c, x, x + 4, 503, 508) for c, x in zip("كمرية", (486, 482, 478, 474, 470), strict=True)],
    ]
    assert reading_order_line(line) == "كمرية (Solid Slab) متر مكعب 45"


def test_brackets_around_a_number_follow_the_arabic_words():
    # "(100 ملم)" drawn right to left: the opening bracket touches 100 on its right.
    line = [
        (")", 0, 2, 0, 8),
        *[(c, x, x + 4, 0, 8) for c, x in zip("ملم", (10, 6, 2), strict=True)],
        (" ", 14, 16, 0, 0),
        *[(c, 16 + 3 * i, 19 + 3 * i, 0, 8) for i, c in enumerate("100")],
        ("(", 25, 27, 0, 8),
        (" ", 27, 29, 0, 0),
        *[
            (c, x, x + 4, 0, 8)
            for c, x in zip("بارتفاع", (53, 49, 45, 41, 37, 33, 29), strict=True)
        ],
    ]
    assert reading_order_line(line) == "بارتفاع (100 ملم)"


def test_a_shadda_stored_apart_goes_over_its_letter():
    line = glyphs("رذاذ", "مرشات") + [(" ", 40, 42, 0, 0), ("ّ", 9, 11, 9, 10)]
    assert reading_order_line(line) == "مرشات رذّاذ"


def test_a_word_stored_in_pieces_is_put_back_in_drawing_order():
    # "معلّقة" stored as "قةّمعل": the last two letters and the shadda first.
    stored = [
        ("ق", 8, 12, 0, 8),
        ("ة", 4, 8, 0, 8),
        ("\u0651", 12, 15, 9, 10),
        ("م", 20, 24, 0, 8),
        ("ع", 16, 20, 0, 8),
        ("ل", 12, 16, 0, 8),
    ]
    assert reading_order_line(stored) == "معل\u0651قة"
    # English inside the same run of characters is never reordered.
    mixed = [("S", 0, 3, 0, 8), ("u", 3, 6, 0, 8), ("m", 6, 9, 0, 8), (")", 9, 10, 0, 8)] + [
        (c, x, x + 4, 0, 8) for c, x in zip("مقطوع", (30, 26, 22, 18, 14), strict=True)
    ]
    assert "Sum" in reading_order_line(mixed)
