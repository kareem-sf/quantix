"""Versioned synthetic retrieval labels. Passages are construction-engineering text.

The six smoke queries keep the wording of scripts/benchmark_multilingual_retrieval.py.
Held-out questions are not used to choose boosts, glossaries or cutoffs.
"""

from __future__ import annotations

from quantix.retrieval_eval import fingerprint

DATASET_VERSION = "2026-09-13.1"

REQUIRED_CATEGORIES = frozenset(
    {
        "clause_numbers",
        "material_grades",
        "arabic_digits",
        "numeric_units",
        "negation_exclusions",
        "heading_clause",
        "table_rows",
        "merged_formula",
        "cross_page",
        "ocr_empty",
        "repeated_areas",
        "revisions",
        "multiple_answers",
        "unrelated",
        "scoped_forbidden",
        "long_work_product",
        "withdrawn_knowledge",
        "public_prices",
        "exhaustive_list",
    }
)

LANGUAGE_PAIRS = frozenset({"en->en", "ar->ar", "ar->en", "en->ar"})

SMOKE_QUERIES = (
    ("pump_en", "أي بند يشترط إنتاجية 70 متراً مكعباً في الساعة لمضخة الخرسانة؟"),
    ("pump_ar", "Which concrete pump requirement specifies 75 cubic metres per hour?"),
    ("waterproof_en", "ما بند غشاء عزل السطح المعدل SBS بسماكة أربعة ملليمترات؟"),
    ("reinforcement_ar", "Which reinforcement clause specifies steel grade B500D?"),
    ("curing_en", "ما البند الذي يشترط معالجة الخرسانة رطباً لمدة سبعة أيام متواصلة؟"),
    ("commercial_ar", "Which brick price clause excludes VAT and includes delivery to Cairo?"),
)

_NOISE_TEXT = "The tenderer shall provide a bid bond of two percent."


def _doc(doc_id, path, text, *, locator="page:1", kind="pdf", status="extracted", extra=None):
    segment = {"locator": locator, "text": text, "page": 1}
    if extra:
        segment.update(extra)
    return {
        "id": doc_id,
        "path": path,
        "kind": kind,
        "status": status,
        "collection": "tender_evidence",
        "segments": [] if status != "extracted" else [segment],
    }


def _passages(doc_id, path, rows, *, kind="pdf"):
    segments = []
    for locator, text, extra in rows:
        page = extra.get("page", 1)
        segment = {"locator": locator, "text": text, "page": page, **extra}
        segments.append(segment)
    return {
        "id": doc_id,
        "path": path,
        "kind": kind,
        "status": "extracted",
        "collection": "tender_evidence",
        "segments": segments,
    }


DOCUMENTS = [
    _doc(
        "pump-en",
        "Plant/pump-en.pdf",
        "The concrete pump shall provide a minimum output of 70 cubic metres per hour at the placing point.",
    ),
    _doc(
        "pump-ar",
        "Plant/pump-ar.pdf",
        "يجب أن توفر مضخة الخرسانة إنتاجية لا تقل عن 75 متراً مكعباً في الساعة عند نقطة الصب.",
    ),
    _doc(
        "waterproof-en",
        "Roof/waterproof-en.pdf",
        "Provide a four millimetre SBS modified bituminous waterproofing membrane to the roof slab.",
    ),
    _doc(
        "reinforcement-ar",
        "Structure/reinforcement-ar.pdf",
        "يجب أن تكون قضبان حديد التسليح من الصنف B500D طبقاً للجدول الإنشائي.",
    ),
    _doc(
        "curing-en",
        "Concrete/curing-en.pdf",
        "Maintain moist curing of structural concrete for at least seven continuous days.",
    ),
    _doc(
        "commercial-ar",
        "Commercial/brick-price-ar.pdf",
        "سعر توريد الطوب لا يشمل ضريبة القيمة المضافة ويشمل النقل إلى موقع المشروع في القاهرة.",
    ),
    _passages(
        "clauses-en",
        "Conditions/payment.pdf",
        [
            (
                "clause:14.7",
                "Clause 14.7 Advance payment. The Employer shall make an advance payment as an interest-free loan for mobilisation.",
                {"page": 14, "heading": "14.7 Advance Payment"},
            ),
            (
                "clause:14.7.1",
                "Clause 14.7.1 The advance payment guarantee shall be issued by a bank acceptable to the Employer.",
                {"page": 14, "heading": "14.7.1 Guarantee"},
            ),
            (
                "clause:8.3.2",
                "Clause 8.3.2 Retention. Five percent of each interim certificate is retained until taking-over.",
                {"page": 8, "heading": "8.3.2 Retention"},
            ),
        ],
    ),
    _passages(
        "grades-en",
        "Structure/grades.pdf",
        [
            (
                "page:2",
                "High-yield reinforcement shall be grade B500B to BS 4449 except where the drawings specify B500D.",
                {"page": 2},
            ),
            (
                "page:3",
                "Foundations use concrete class C30/37. Walls above ground use C35/45.",
                {"page": 3},
            ),
            (
                "page:4",
                "Structural steel beams shall be grade S355. S275 is not permitted.",
                {"page": 4},
            ),
        ],
    ),
    _doc(
        "cover-ar",
        "Structure/cover-ar.pdf",
        "الغطاء الخرساني للتسليح لا يقل عن ٢٥ مم للعناصر الداخلية و ٤٠ مم للعناصر المعرضة للتربة.",
    ),
    _doc(
        "arabic-days",
        "Concrete/curing-ar.pdf",
        "تستمر المعالجة الرطبة للخرسانة الإنشائية مدة لا تقل عن ٧ أيام متواصلة، أي ما يعادل سبعة أيام.",
    ),
    _passages(
        "units-en",
        "Structure/cover-units.pdf",
        [
            (
                "page:1",
                "Minimum concrete cover to reinforcement is 20 mm on internal faces.",
                {"page": 1},
            ),
            ("page:2", "The retaining wall length is 20 m between expansion joints.", {"page": 2}),
            (
                "page:3",
                "The slurry pump duty is 70 litres per second. This is not a concrete placing pump.",
                {"page": 3},
            ),
        ],
    ),
    _doc(
        "exclusion-hose",
        "Plant/pump-exclusions.pdf",
        "The concrete pump hire rate includes the placing boom. The delivery hose is excluded from the rate.",
    ),
    _doc(
        "exclusion-vat-en",
        "Commercial/rates-vat.pdf",
        "Unit rates exclude VAT. Transport to the Cairo site is included.",
    ),
    _passages(
        "fire-en",
        "Architecture/fire.pdf",
        [
            (
                "heading:1",
                "Fire stopping",
                {"page": 6, "heading": "Fire stopping", "block_kind": "heading"},
            ),
            (
                "page:6",
                "Doorsets in the escape corridor shall provide 60 minutes fire resistance. This clause does not specify cavity barriers.",
                {"page": 6, "heading": "Fire doors"},
            ),
        ],
    ),
    _passages(
        "boq-table",
        "Estimate/boq-concrete.pdf",
        [
            (
                "row:3.1",
                "Item 3.1 Blinding concrete grade C15 under foundations. Quantity 12.5. Unit m3. Rate 850.",
                {"page": 1},
            ),
            (
                "row:3.2",
                "Item 3.2 Blinding concrete grade C15 under ground beams. Quantity 4.0. Unit m3. Rate 850.",
                {"page": 1},
            ),
            ("header:1", "Columns: Item, Description, Quantity, Unit, Rate.", {"page": 1}),
        ],
    ),
    _passages(
        "spreadsheet-warn",
        "Estimate/boq.xlsx",
        [
            (
                "cell:E12",
                "Sheet BOQ cell E12 formula=#REF! cached=none warning=broken_reference item=3.12",
                {"page": 1, "sheet": "BOQ", "cell_range": "E12"},
            ),
            (
                "cell:A1:C1",
                "Sheet BOQ merged A1:C1 text=Section 3 Concrete",
                {"page": 1, "sheet": "BOQ", "cell_range": "A1:C1"},
            ),
            (
                "cell:D12",
                "Sheet BOQ D12 cached_value=12.5 formula=C12*1.0 item=3.12 quantity",
                {"page": 1, "sheet": "BOQ", "cell_range": "D12"},
            ),
        ],
        kind="xlsx",
    ),
    _passages(
        "cross-page",
        "Specification/waterproofing.pdf",
        [
            (
                "page:3",
                "Unless stated otherwise in the following clause, membranes are torch-applied. Do not use cold adhesive on occupied roofs.",
                {"page": 3},
            ),
            (
                "page:4",
                "Provide the roof waterproofing membrane specified on this page. Application method is as stated on the previous page.",
                {"page": 4},
            ),
            (
                "page:5",
                "Exception: planter upstands may use cold adhesive where torch work is unsafe.",
                {"page": 5},
            ),
        ],
    ),
    {
        "id": "drawing-ocr",
        "path": "Drawings/roof-plan.dwg",
        "kind": "dwg",
        "status": "unsupported",
        "collection": "tender_evidence",
        "segments": [],
    },
    {
        "id": "scan-empty",
        "path": "Specification/scanned-addendum.pdf",
        "kind": "pdf",
        "status": "needs_attention",
        "collection": "tender_evidence",
        "segments": [{"locator": "page:1", "text": "", "page": 1}],
    },
    _doc(
        "spec-civil",
        "Civil/waterproofing-spec.pdf",
        "Provide a four millimetre SBS modified bituminous waterproofing membrane to the roof slab. Civil copy.",
    ),
    _doc(
        "spec-arch",
        "Architecture/waterproofing-spec.pdf",
        "Provide a four millimetre SBS modified bituminous waterproofing membrane to the roof slab. Architecture copy.",
    ),
    _doc(
        "curing-second",
        "Concrete/curing-cubes.pdf",
        "Keep cube specimens under moist curing for seven days before testing compressive strength.",
    ),
    _doc(
        "bond-permitted",
        "Permitted/conditions.pdf",
        "The contractor shall provide a bid bond of two percent of the contract sum in the form of a bank guarantee.",
    ),
    _doc(
        "item-code",
        "Estimate/item-03-12-04.pdf",
        "Item 03.12.04 Supply and place crushed stone sub-base, 150 mm compacted thickness.",
    ),
    _doc(
        "pumping-rate",
        "Commercial/pumping-excluded.pdf",
        "The concrete unit rate excludes pumping. Placing by crane and skip is included.",
    ),
    _doc(
        "cover-25",
        "Structure/cover-25.pdf",
        "Minimum cover to reinforcement in mild exposure is 25 mm unless the drawings show a greater cover.",
    ),
    _doc(
        "advance-ar",
        "Conditions/advance-ar.pdf",
        "الدفعة المقدمة تُصرف كقرض بدون فائدة للتحضير، مقابل خطاب ضمان بنكي مقبول لدى صاحب العمل.",
    ),
    *[
        _doc(
            f"noise-bond-{index:02d}",
            f"Noise/bid-bond-{index:02d}.pdf",
            _NOISE_TEXT,
        )
        for index in range(20)
    ],
]

NON_SOURCE_RECORDS = [
    {
        "id": "wp-long-report",
        "collection": "work_product",
        "locator": "char:16000",
        "text": "Saved draft report. The delayed paragraph after character 16000 states the bid validity is 120 days.",
    },
    {
        "id": "knowledge-vat",
        "collection": "knowledge",
        "status": "withdrawn",
        "text": "Treat all rates as VAT inclusive unless the source says otherwise.",
    },
    {
        "id": "knowledge-recheck",
        "collection": "knowledge",
        "status": "recheck_due",
        "text": "Company note: typical Cairo brick haulage was 180 EGP per thousand in 2024. Recheck before reuse.",
    },
    {
        "id": "research-rebar",
        "collection": "research",
        "fetched": "2026-01-15",
        "text": "Public observation: 16 mm B500B rebar offered at 41,000 EGP per tonne on 15 January 2026.",
    },
    {
        "id": "research-diesel",
        "collection": "research",
        "fetched": "2025-06-01",
        "text": "Public observation: diesel 12.50 EGP per litre on 1 June 2025. Dated; not a current Tender rate.",
    },
]


def _q(
    qid,
    category,
    query,
    relevant,
    *,
    split="train",
    qlang="en",
    plang="en",
    absent=False,
    permitted=None,
    collection="tender_evidence",
    exhaustive=False,
    smoke=False,
    context=None,
    phase="base",
):
    spans = []
    for item in relevant:
        if isinstance(item, tuple):
            document, locator, *rest = item
            grade = rest[0] if rest else 2
            spans.append({"document": document, "locator": locator, "grade": grade})
        else:
            spans.append(item)
    return {
        "id": qid,
        "split": split,
        "category": category,
        "query": query,
        "query_language": qlang,
        "passage_language": plang,
        "relevant": spans,
        "absent": absent,
        "permitted_documents": permitted,
        "collection": collection,
        "exhaustive": exhaustive,
        "smoke": smoke,
        "required_context": context,
        "phase": phase,
    }


QUESTIONS = [
    _q(
        "smoke-pump-en",
        "numeric_units",
        SMOKE_QUERIES[0][1],
        [("pump-en", "page:1")],
        qlang="ar",
        plang="en",
        smoke=True,
    ),
    _q(
        "smoke-pump-ar",
        "numeric_units",
        SMOKE_QUERIES[1][1],
        [("pump-ar", "page:1")],
        qlang="en",
        plang="ar",
        smoke=True,
    ),
    _q(
        "smoke-waterproof",
        "numeric_units",
        SMOKE_QUERIES[2][1],
        [("waterproof-en", "page:1")],
        qlang="ar",
        plang="en",
        smoke=True,
    ),
    _q(
        "smoke-rebar",
        "material_grades",
        SMOKE_QUERIES[3][1],
        [("reinforcement-ar", "page:1")],
        qlang="en",
        plang="ar",
        smoke=True,
    ),
    _q(
        "smoke-curing",
        "numeric_units",
        SMOKE_QUERIES[4][1],
        [("curing-en", "page:1")],
        qlang="ar",
        plang="en",
        smoke=True,
    ),
    _q(
        "smoke-vat",
        "negation_exclusions",
        SMOKE_QUERIES[5][1],
        [("commercial-ar", "page:1")],
        qlang="en",
        plang="ar",
        smoke=True,
    ),
    _q(
        "cl-14-7",
        "clause_numbers",
        "What does clause 14.7 require?",
        [("clauses-en", "clause:14.7")],
    ),
    _q(
        "cl-14-7-1",
        "clause_numbers",
        "What bank instrument does clause 14.7.1 require?",
        [("clauses-en", "clause:14.7.1")],
    ),
    _q(
        "cl-14-7-ar",
        "clause_numbers",
        "ماذا يشترط البند 14.7؟",
        [("clauses-en", "clause:14.7")],
        qlang="ar",
        plang="en",
    ),
    _q(
        "cl-8-3-2",
        "clause_numbers",
        "What retention does clause 8.3.2 set?",
        [("clauses-en", "clause:8.3.2")],
        split="held_out",
    ),
    _q(
        "gr-b500d",
        "material_grades",
        "Where is reinforcement grade B500D specified?",
        [("grades-en", "page:2"), ("reinforcement-ar", "page:1", 1)],
    ),
    _q(
        "gr-c30",
        "material_grades",
        "Which concrete class is specified for foundations?",
        [("grades-en", "page:3")],
    ),
    _q(
        "gr-b500d-ar",
        "material_grades",
        "أين يذكر صنف حديد B500D؟",
        [("reinforcement-ar", "page:1"), ("grades-en", "page:2", 1)],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "gr-s355",
        "material_grades",
        "Which steel grade is required for beams?",
        [("grades-en", "page:4")],
        split="held_out",
    ),
    _q(
        "ar-cover-25",
        "arabic_digits",
        "ما الغطاء الخرساني الداخلي المطلوب؟",
        [("cover-ar", "page:1")],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "ar-seven-days",
        "arabic_digits",
        "كم يوماً تستمر المعالجة الرطبة؟",
        [("arabic-days", "page:1"), ("curing-en", "page:1", 1)],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "ar-12-as-indic",
        "arabic_digits",
        "What internal cover is written with Arabic-Indic digits?",
        [("cover-ar", "page:1")],
        qlang="en",
        plang="ar",
    ),
    _q(
        "ar-25mm",
        "arabic_digits",
        "What cover is twenty-five millimetres in Arabic digits?",
        [("cover-ar", "page:1")],
        qlang="en",
        plang="ar",
        split="held_out",
    ),
    _q(
        "nu-20mm",
        "numeric_units",
        "What is the minimum internal concrete cover in millimetres?",
        [("units-en", "page:1")],
    ),
    _q(
        "nu-20m",
        "numeric_units",
        "How long is the retaining wall between expansion joints?",
        [("units-en", "page:2")],
    ),
    _q(
        "nu-70ls",
        "numeric_units",
        "Which pump is specified at 70 litres per second?",
        [("units-en", "page:3")],
    ),
    _q(
        "nu-70-vs-75",
        "numeric_units",
        "Which clause sets concrete pump output at 70 cubic metres per hour?",
        [("pump-en", "page:1")],
    ),
    _q(
        "nu-0-20",
        "numeric_units",
        "What wall length is twenty metres, not twenty millimetres?",
        [("units-en", "page:2")],
        split="held_out",
    ),
    _q(
        "neg-hose",
        "negation_exclusions",
        "Is the delivery hose included in the concrete pump hire rate?",
        [("exclusion-hose", "page:1")],
    ),
    _q(
        "neg-vat-en",
        "negation_exclusions",
        "Do the unit rates include VAT?",
        [("exclusion-vat-en", "page:1")],
    ),
    _q(
        "neg-vat-ar",
        "negation_exclusions",
        "هل سعر الطوب يشمل ضريبة القيمة المضافة؟",
        [("commercial-ar", "page:1")],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "neg-pumping",
        "negation_exclusions",
        "Does the concrete unit rate include pumping?",
        [("pumping-rate", "page:1")],
        split="held_out",
    ),
    _q(
        "hd-fire-doors",
        "heading_clause",
        "What fire resistance is required for escape corridor doorsets?",
        [("fire-en", "page:6")],
        context={"type": "heading", "document": "fire-en", "locator": "heading:1"},
    ),
    _q(
        "hd-fire-stopping",
        "heading_clause",
        "Fire stopping",
        [("fire-en", "heading:1")],
        context={"type": "heading", "document": "fire-en", "locator": "heading:1"},
    ),
    _q(
        "hd-waterproof-body",
        "heading_clause",
        "What membrane is specified for the roof slab?",
        [("waterproof-en", "page:1")],
    ),
    _q(
        "hd-placing",
        "heading_clause",
        "Which clause sits under placing plant rather than lifting plant?",
        [("pump-en", "page:1")],
        split="held_out",
    ),
    _q(
        "tb-3-1",
        "table_rows",
        "What quantity is shown for item 3.1 blinding?",
        [("boq-table", "row:3.1")],
    ),
    _q(
        "tb-3-2",
        "table_rows",
        "What quantity is shown for item 3.2 blinding under ground beams?",
        [("boq-table", "row:3.2")],
    ),
    _q(
        "tb-header",
        "table_rows",
        "Which BOQ columns are listed for the concrete bill?",
        [("boq-table", "header:1")],
    ),
    _q(
        "tb-3-2-rate",
        "table_rows",
        "What rate is written against item 3.2?",
        [("boq-table", "row:3.2")],
        split="held_out",
    ),
    _q(
        "mf-ref",
        "merged_formula",
        "Which BOQ cell has a broken #REF! formula?",
        [("spreadsheet-warn", "cell:E12")],
    ),
    _q(
        "mf-merged",
        "merged_formula",
        "What merged header does section 3 use on the BOQ sheet?",
        [("spreadsheet-warn", "cell:A1:C1")],
    ),
    _q(
        "mf-cached",
        "merged_formula",
        "What cached quantity sits in BOQ cell D12?",
        [("spreadsheet-warn", "cell:D12")],
        split="held_out",
    ),
    _q(
        "xp-method",
        "cross_page",
        "How must the roof waterproofing membrane on page 4 be applied?",
        [("cross-page", "page:4"), ("cross-page", "page:3", 1)],
        context={"type": "previous_page", "document": "cross-page", "locator": "page:3"},
    ),
    _q(
        "xp-occupied",
        "cross_page",
        "May cold adhesive be used on occupied roofs?",
        [("cross-page", "page:3")],
    ),
    _q(
        "xp-planter",
        "cross_page",
        "Where is cold adhesive allowed as an exception?",
        [("cross-page", "page:5")],
        split="held_out",
    ),
    _q(
        "ocr-drawing",
        "ocr_empty",
        "What notes are written on the roof plan drawing?",
        [],
        absent=True,
    ),
    _q(
        "ocr-scan",
        "ocr_empty",
        "What does the scanned addendum say about completion?",
        [],
        absent=True,
    ),
    _q(
        "ocr-scan-held",
        "ocr_empty",
        "Extract the text of the unreadable scanned addendum.",
        [],
        absent=True,
        split="held_out",
    ),
    _q(
        "rp-civil",
        "repeated_areas",
        "Where does the Civil specification copy the 4 mm SBS roof membrane?",
        [("spec-civil", "page:1")],
    ),
    _q(
        "rp-arch",
        "repeated_areas",
        "Where does the Architecture specification copy the 4 mm SBS roof membrane?",
        [("spec-arch", "page:1")],
    ),
    _q(
        "rp-both",
        "repeated_areas",
        "Which two area copies specify the four millimetre SBS roof membrane?",
        [("spec-civil", "page:1"), ("spec-arch", "page:1")],
    ),
    _q(
        "rp-civil-held",
        "repeated_areas",
        "In the Civil folder, what roof membrane thickness is specified?",
        [("spec-civil", "page:1")],
        split="held_out",
        permitted=["spec-civil"],
    ),
    _q(
        "rev-82",
        "revisions",
        "أي بند يشترط إنتاجية 82 متراً مكعباً في الساعة لمضخة الخرسانة؟",
        [("pump-en", "page:1")],
        qlang="ar",
        plang="en",
        phase="after_revision",
        split="held_out",
    ),
    _q(
        "rev-stale-70",
        "revisions",
        "What current minimum concrete pump output replaced 70 cubic metres per hour?",
        [("pump-en", "page:1")],
        phase="after_revision",
    ),
    _q(
        "rev-current-en",
        "revisions",
        "What current minimum concrete pump output is specified after the file revision?",
        [("pump-en", "page:1")],
        phase="after_revision",
    ),
    _q(
        "ma-curing",
        "multiple_answers",
        "Which clauses require seven days of moist curing?",
        [("curing-en", "page:1"), ("curing-second", "page:1"), ("arabic-days", "page:1", 1)],
    ),
    _q(
        "ma-bond",
        "multiple_answers",
        "Where is a bid bond of two percent specified for the contractor with a bank guarantee?",
        [("bond-permitted", "page:1")],
    ),
    _q(
        "ma-b500",
        "multiple_answers",
        "Which sources mention B500D reinforcement?",
        [("reinforcement-ar", "page:1"), ("grades-en", "page:2")],
        split="held_out",
    ),
    _q("un-neptune", "unrelated", "What is the orbital period of Neptune?", [], absent=True),
    _q("un-worldcup", "unrelated", "Who won the FIFA World Cup in 2022?", [], absent=True),
    _q(
        "un-nitrogen",
        "unrelated",
        "What is the boiling point of nitrogen?",
        [],
        absent=True,
        split="held_out",
    ),
    _q(
        "sc-bond",
        "scoped_forbidden",
        "bid bond bank guarantee",
        [("bond-permitted", "page:1")],
        permitted=["bond-permitted"],
    ),
    _q(
        "sc-bond-ar",
        "scoped_forbidden",
        "خطاب ضمان ابتدائي بنسبة اثنين بالمائة",
        [("bond-permitted", "page:1")],
        qlang="ar",
        plang="en",
        permitted=["bond-permitted"],
    ),
    _q(
        "sc-bond-held",
        "scoped_forbidden",
        "contractor bid bond two percent bank guarantee",
        [("bond-permitted", "page:1")],
        permitted=["bond-permitted"],
        split="held_out",
    ),
    _q(
        "wp-validity",
        "long_work_product",
        "What bid validity is stated after character 16000 in the saved draft report?",
        [("wp-long-report", "char:16000")],
        collection="work_product",
    ),
    _q(
        "wp-validity-held",
        "long_work_product",
        "Find the 120-day validity sentence in the long saved report.",
        [("wp-long-report", "char:16000")],
        collection="work_product",
        split="held_out",
    ),
    _q(
        "kn-withdrawn",
        "withdrawn_knowledge",
        "Should rates be treated as VAT inclusive by default from the company note?",
        [("knowledge-vat", "note:1")],
        collection="knowledge",
        absent=True,
    ),
    _q(
        "kn-recheck",
        "withdrawn_knowledge",
        "What 2024 Cairo brick haulage figure is stored in company knowledge?",
        [("knowledge-recheck", "note:1")],
        collection="knowledge",
        split="held_out",
    ),
    _q(
        "pr-rebar",
        "public_prices",
        "What dated public price was observed for 16 mm B500B rebar?",
        [("research-rebar", "passage:1")],
        collection="research",
    ),
    _q(
        "pr-diesel",
        "public_prices",
        "What dated public diesel price is on file?",
        [("research-diesel", "passage:1")],
        collection="research",
        split="held_out",
    ),
    _q(
        "ex-requirements",
        "exhaustive_list",
        "Find every requirement in this package.",
        [],
        exhaustive=True,
    ),
    _q(
        "ex-exclusions",
        "exhaustive_list",
        "List all exclusions from rates and hire.",
        [
            ("exclusion-hose", "page:1"),
            ("pumping-rate", "page:1"),
            ("commercial-ar", "page:1"),
            ("exclusion-vat-en", "page:1"),
        ],
        exhaustive=True,
    ),
    _q(
        "ex-dates",
        "exhaustive_list",
        "List every submission date in the package.",
        [],
        exhaustive=True,
        absent=True,
        split="held_out",
    ),
    _q(
        "id-03-12-04",
        "clause_numbers",
        "What does item 03.12.04 describe?",
        [("item-code", "page:1")],
    ),
    _q(
        "id-filename",
        "clause_numbers",
        "Which roof file specifies the SBS membrane?",
        [("waterproof-en", "page:1")],
    ),
    _q(
        "gr-b500b-not-d",
        "material_grades",
        "Where is B500B the default high-yield grade?",
        [("grades-en", "page:2")],
    ),
    _q(
        "pump-ar-ar",
        "numeric_units",
        "ما إنتاجية مضخة الخرسانة المطلوبة بالمتر المكعب في الساعة؟",
        [("pump-ar", "page:1")],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "waterproof-en-en",
        "numeric_units",
        "What thickness of SBS roof membrane is specified?",
        [("waterproof-en", "page:1")],
    ),
    _q(
        "cairo-delivery",
        "negation_exclusions",
        "Does the brick supply price include transport to Cairo?",
        [("commercial-ar", "page:1")],
        qlang="en",
        plang="ar",
    ),
    _q(
        "seven-days-en",
        "numeric_units",
        "How many continuous days of moist curing are required for structural concrete?",
        [("curing-en", "page:1")],
    ),
    _q(
        "sbs-ar",
        "numeric_units",
        "ما سماكة غشاء العزل SBS للسطح؟",
        [("waterproof-en", "page:1")],
        qlang="ar",
        plang="en",
    ),
    _q("cl-14-8-absent", "clause_numbers", "What does clause 14.8 require?", [], absent=True),
    _q(
        "gr-c25-absent",
        "material_grades",
        "Where is concrete class C25/30 specified for foundations?",
        [],
        absent=True,
    ),
    _q(
        "cover-25-en",
        "numeric_units",
        "What minimum cover is specified for mild exposure?",
        [("cover-25", "page:1")],
    ),
    _q(
        "advance-guarantee-en",
        "clause_numbers",
        "Which clause requires an advance payment guarantee from an acceptable bank?",
        [("clauses-en", "clause:14.7.1")],
    ),
    _q(
        "advance-ar-q",
        "clause_numbers",
        "ما ضمان الدفعة المقدمة؟",
        [("advance-ar", "page:1"), ("clauses-en", "clause:14.7.1", 1)],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "boost-pump-ar",
        "numeric_units",
        "قدرة مضخة الخرسانة",
        [("pump-en", "page:1"), ("pump-ar", "page:1", 1)],
        qlang="ar",
        plang="en",
    ),
    _q(
        "boost-capacity",
        "numeric_units",
        "قدرة الانتاج لمضخة الخرسانة",
        [("pump-ar", "page:1"), ("pump-en", "page:1", 1)],
        qlang="ar",
        plang="ar",
    ),
    _q(
        "item-3-12-sheet",
        "merged_formula",
        "Which spreadsheet item is tied to the broken formula cell?",
        [("spreadsheet-warn", "cell:E12")],
    ),
    _q(
        "b500b-ar",
        "material_grades",
        "أين يُذكر الحديد B500B؟",
        [("grades-en", "page:2")],
        qlang="ar",
        plang="en",
        split="held_out",
    ),
    _q(
        "hose-ar",
        "negation_exclusions",
        "هل خرطوم التوريد مشمول في سعر مضخة الخرسانة؟",
        [("exclusion-hose", "page:1")],
        qlang="ar",
        plang="en",
        split="held_out",
    ),
    _q(
        "subbase",
        "numeric_units",
        "What compacted thickness is specified for crushed stone sub-base item 03.12.04?",
        [("item-code", "page:1")],
        split="held_out",
    ),
]


def corpus_payload() -> dict:
    return {
        "dataset_version": DATASET_VERSION,
        "documents": DOCUMENTS,
        "questions": QUESTIONS,
        "non_source_records": NON_SOURCE_RECORDS,
    }


def corpus_hash() -> str:
    return fingerprint(corpus_payload())


def validate_dataset() -> dict:
    ids = [question["id"] for question in QUESTIONS]
    if len(ids) != len(set(ids)):
        raise ValueError("Question identifiers must be unique.")
    if len(QUESTIONS) < 80:
        raise ValueError("The labeled set must contain at least 80 questions.")
    categories = {question["category"] for question in QUESTIONS}
    missing = REQUIRED_CATEGORIES - categories
    if missing:
        raise ValueError(f"Missing required categories: {sorted(missing)}")
    held = [question for question in QUESTIONS if question["split"] == "held_out"]
    train = [question for question in QUESTIONS if question["split"] == "train"]
    if not held or {question["id"] for question in held} & {question["id"] for question in train}:
        raise ValueError("Held-out questions must be a distinct nonempty set.")
    held_categories = {question["category"] for question in held}
    if held_categories != REQUIRED_CATEGORIES:
        raise ValueError(
            "Held-out questions must include every required category; missing "
            f"{sorted(REQUIRED_CATEGORIES - held_categories)}."
        )
    pairs = {
        f"{question['query_language']}->{question['passage_language']}" for question in QUESTIONS
    }
    if not LANGUAGE_PAIRS <= pairs:
        raise ValueError(f"Missing language pairs: {sorted(LANGUAGE_PAIRS - pairs)}")
    smoke = [question for question in QUESTIONS if question["smoke"]]
    if [question["query"] for question in smoke] != [item[1] for item in SMOKE_QUERIES]:
        raise ValueError("Smoke queries must keep the original six-query wording.")
    documents = {item["id"]: item for item in DOCUMENTS}
    for question in QUESTIONS:
        if question["collection"] != "tender_evidence":
            continue
        for span in question["relevant"]:
            document = documents.get(span["document"])
            if document is None:
                raise ValueError(f"Unknown document {span['document']} on {question['id']}.")
            locators = {segment["locator"] for segment in document["segments"]}
            if span["locator"] not in locators:
                raise ValueError(
                    f"{question['id']} locator {span['locator']} is missing from {span['document']}."
                )
    return {
        "dataset_version": DATASET_VERSION,
        "corpus_hash": corpus_hash(),
        "questions": len(QUESTIONS),
        "held_out": len(held),
        "train": len(train),
        "documents": len(DOCUMENTS),
        "categories": sorted(categories),
    }


validate_dataset()
