import random

from quantix.documents.arabic import Char, has_arabic, lines_from_boxes, searchable

W = 6.0  # width of one drawn character


def draw_line(words: list[str], y: float = 700.0) -> list[Char]:
    """Draw a reading-order line the way a page shows it: Arabic words from the right, numbers left to right."""
    chars: list[Char] = []
    x = 500.0  # right edge; each word is placed to the left of the previous one
    runs: list[list[str]] = []
    for word in words:  # consecutive non-Arabic words read left to right as one run
        if not has_arabic(word) and runs and not has_arabic(runs[-1][0]):
            runs[-1].append(word)
        else:
            runs.append([word])
    for run in runs:
        if has_arabic(run[0]):
            for letter in run[0]:  # first letter is drawn rightmost
                x -= W
                chars.append(Char(letter, x, y, x + W, y + 10))
            x -= W  # the space
            continue
        width = sum(len(w) for w in run) * W + (len(run) - 1) * W
        left = x - width
        for i, word in enumerate(run):
            for letter in word:
                chars.append(Char(letter, left, y, left + W, y + 10))
                left += W
            if i < len(run) - 1:
                left += W
        x -= width + W
    return chars


def test_arabic_line_reads_right_to_left_whatever_the_storage_order():
    line = draw_line(["الكمية", "45", "متر", "مكعب"])
    for stored in (line, list(reversed(line)), random.Random(7).sample(line, len(line))):
        assert lines_from_boxes(stored) == ["الكمية 45 متر مكعب"]


def test_english_runs_stay_left_to_right_inside_arabic():
    line = draw_line(["خرسانة", "C35", "grade", "حسب", "المواصفات"])
    assert lines_from_boxes(line) == ["خرسانة C35 grade حسب المواصفات"]


def test_a_percentage_reads_number_first():
    line = draw_line(["بنسبة", "%1", "من", "القيمة"])
    assert lines_from_boxes(line) == ["بنسبة 1% من القيمة"]


def test_vowel_marks_stay_with_their_letter():
    word = "مُعلّقة"
    chars = draw_line([word.replace("ُ", "").replace("ّ", "")])
    # The marks are drawn over their letters: damma over mim (1st), shadda over lam (3rd).
    by_letter = {c.text: c for c in chars}
    chars.append(Char("ُ", by_letter["م"].left + 1, 700, by_letter["م"].right - 1, 712))
    chars.append(Char("ّ", by_letter["ل"].left + 1, 700, by_letter["ل"].right - 1, 712))
    assert lines_from_boxes(chars) == [word]


def test_a_lam_alef_ligature_reads_lam_first_whichever_letter_is_stored_first():
    """A ligature comes as two letters in one box; the tender's specification stores the alef first, which read
    "خالل" for "خلال" and "األعمال" for "الأعمال". The article "ال" is two boxes, and stays as drawn."""
    line = draw_line(["خلال", "الأعمال"])
    for ligature in ("لا", "لأ"):
        at = next(i for i, c in enumerate(line) if c.text == ligature[0] and line[i + 1].text == ligature[1])
        lam, alef = line[at], line[at + 1]
        box = (alef.left, alef.bottom, lam.right, lam.top)
        line[at : at + 2] = [Char(alef.text, *box), Char(lam.text, *box)]  # alef stored first, both in one box
    for stored in (line, list(reversed(line))):
        assert lines_from_boxes(stored) == ["خلال الأعمال"]


def test_lines_come_top_first_and_english_is_untouched():
    page = draw_line(["Bill", "of", "Quantities"], y=700) + draw_line(["جدول", "الكميات"], y=680)
    assert lines_from_boxes(page) == ["Bill of Quantities", "جدول الكميات"]


def test_search_ignores_vowel_marks_and_letter_variants():
    assert searchable("مُعلّقة") == searchable("معلقه")
    assert searchable("إنشاء") == searchable("انشاء")
    assert searchable("Concrete") == "concrete"


def test_a_real_arabic_pdf_reads_in_order():
    """A synthetic PDF written with real text shaping (fpdf2 + HarfBuzz, Arial); PDFium stores its words backwards."""
    from pathlib import Path

    from quantix.documents.readers import read_file

    pages = read_file(Path(__file__).parent / "fixtures" / "synthetic-arabic.pdf", "pdf")
    assert pages[0].text.splitlines() == [
        "Bill of Quantities - Block B",
        "الكمية 45 متر مكعب",
        "خرسانة C35 حسب المواصفات",
        "ضمان ابتدائي بنسبة 1% من قيمة العطاء",
    ]
    assert pages[1].has_text is False


def test_characters_beyond_the_basic_plane_are_read_whole(monkeypatch):
    """PDFium gives a mathematical letter like 𝑅 as two UTF-16 halves; kept apart, the page couldn't be saved."""
    from quantix.documents import readers

    text = "القيمة 𝑅"
    codes = [unit for ch in text for unit in _utf16(ch)]

    class Page:
        raw = None

        def get_text_bounded(self):
            return text

        def count_chars(self):
            return len(codes)

        def get_charbox(self, index, loose=False):
            return (500.0 - 6 * index, 700.0, 506.0 - 6 * index, 710.0)  # drawn right to left

    monkeypatch.setattr(readers.pdfium_raw, "FPDFText_GetUnicode", lambda _, index: codes[index])
    assert readers._pdf_page_text(Page()) == "القيمة R"  # whole, then as plain a letter as search needs


def _utf16(char: str) -> list[int]:
    data = char.encode("utf-16-le")
    return [int.from_bytes(data[i : i + 2], "little") for i in range(0, len(data), 2)]


def test_a_word_whose_letters_share_one_box_keeps_the_order_it_was_stored_in():
    """Some PDFs give every letter of a word the same box; only a box of exactly lam and alef is a ligature."""
    word = "الطول"
    box = (400.0, 700.0, 430.0, 710.0)
    assert lines_from_boxes([Char(letter, *box) for letter in word]) == [word]
