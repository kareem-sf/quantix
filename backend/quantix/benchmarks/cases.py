"""Twenty-four independently authored synthetic goldens. No production calculations.

All expected numbers below are literal reviewed examples, not values generated
by the calculator being tested. Drawing cases are revision notices/dimension
schedules, not a claim to qualify visual geometry interpretation.
"""

from .models import BenchmarkCase

DATASET_VERSION = "2026-09-13.1"


def _case(
    identifier,
    category,
    title,
    instruction,
    sources,
    numbers,
    finding,
    probe,
    *,
    forbidden=(),
    check=False,
):
    return BenchmarkCase(
        id=identifier,
        category=category,
        title=title,
        instruction=instruction,
        sources=[
            {"id": identifier + "-" + key, "title": heading, "text": text}
            for key, heading, text in sources
        ],
        numbers={
            key: {
                "value": value,
                "unit": unit,
                "evidence": [identifier + "-" + ref for ref in refs],
            }
            for key, value, unit, refs in numbers
        },
        findings=[
            {"code": code, "evidence": [identifier + "-" + ref for ref in refs]}
            for code, refs in finding
        ],
        forbidden_findings=list(forbidden),
        required_assignments=1 if check else 0,
        required_events=["calculation_completed", "saved_work_product"]
        + (["calculation_checked"] if check else []),
        probe={"method": probe[0], "inputs": probe[1], "units": probe[2], "expected": probe[3]},
    )


CASES = [
    _case(
        "boq-01",
        "boq",
        "Slab volume discrepancy",
        "Recompute the slab volume and report the signed difference from the BOQ.",
        [
            ("A", "Slab dimensions", "Synthetic slab: length 10 m, width 5 m, thickness 0.20 m."),
            ("B", "BOQ", "BOQ concrete quantity: 12 m3. Difference means calculated minus BOQ."),
        ],
        [("quantity", "10", "m3", ["A"]), ("difference", "-2", "m3", ["A", "B"])],
        [("quantity_discrepancy", ["A", "B"])],
        (
            "product",
            {"quantity": "50", "factor": "0.20"},
            {"quantity": "m2", "factor": "m"},
            {"product": "10.00", "unit": "m3"},
        ),
    ),
    _case(
        "boq-02",
        "boq",
        "Deduct wall openings",
        "Compute net measured wall area. Apply the stated full-opening deduction rule.",
        [
            (
                "A",
                "Wall schedule",
                "Wall length 30 m; height 3 m. Two openings, each 1.5 m wide and 2 m high.",
            ),
            (
                "B",
                "Measurement rule",
                "Deduct the full area of both openings. No other deductions.",
            ),
        ],
        [("net_area", "84", "m2", ["A", "B"])],
        [("deduct_openings", ["A", "B"])],
        (
            "difference",
            {"left": "90", "right": "6"},
            {"left": "m2", "right": "m2"},
            {"difference": "84.00", "unit": "m2"},
        ),
    ),
    _case(
        "boq-03",
        "boq",
        "Reinforcement amount",
        "Compute the reinforcement amount, retaining the quoted tonne unit and tax basis.",
        [
            (
                "A",
                "Reinforcement BOQ",
                "Quantity 2.5 t; rate EGP 42000 per t, excluding VAT. Do not add waste or tax.",
            )
        ],
        [("amount", "105000", "EGP", ["A"])],
        [("tax_excluded", ["A"])],
        (
            "product",
            {"quantity": "2.5", "factor": "42000"},
            {},
            {"product": "105000.00", "unit": "1"},
        ),
    ),
    _case(
        "boq-04",
        "boq",
        "Exclude duplicated line",
        "Price the two distinct pipe items. Identify the duplicate informational copy.",
        [
            (
                "A",
                "Pipe BOQ",
                "Item P1: 100 m at EGP 250/m. Item P2: 40 m at EGP 500/m. P1-COPY repeats P1 and is explicitly informational, not an additional quantity.",
            )
        ],
        [("total", "45000", "EGP", ["A"])],
        [("duplicate_excluded", ["A"])],
        ("sum", {"values": ["25000", "20000"]}, {}, {"sum": "45000.00", "unit": "1"}),
    ),
    _case(
        "spec-01",
        "specification_conflict",
        "Unresolved concrete grade",
        "Identify the incompatible concrete requirements and their numerical spread. Do not invent precedence.",
        [
            ("A", "Structural specification", "Required concrete characteristic strength: 30 MPa."),
            (
                "B",
                "Structural drawing note",
                "The same concrete element requires 40 MPa. No precedence document has been supplied.",
            ),
        ],
        [("spread", "10", "MPa", ["A", "B"])],
        [("specification_conflict", ["A", "B"]), ("clarification_required", ["A", "B"])],
        ("difference", {"left": "40", "right": "30"}, {}, {"difference": "10.00", "unit": "1"}),
        forbidden=["governing_requirement_selected"],
    ),
    _case(
        "spec-02",
        "specification_conflict",
        "Signed fire-door addendum",
        "Apply the supplied signed addendum and identify the superseded door requirement.",
        [
            ("A", "Original door schedule", "Door FD1 fire rating: 90 minutes."),
            (
                "B",
                "Signed addendum 02",
                "For FD1, replace 90 minutes with 120 minutes. This signed addendum explicitly takes precedence over the original schedule.",
            ),
        ],
        [("governing_rating", "120", "min", ["B"]), ("increase", "30", "min", ["A", "B"])],
        [("addendum_precedence", ["A", "B"])],
        ("difference", {"left": "120", "right": "90"}, {}, {"difference": "30.00", "unit": "1"}),
    ),
    _case(
        "spec-03",
        "specification_conflict",
        "Waterproofing build-up conflict",
        "Compare total specified membrane thicknesses and retain the unresolved contradiction.",
        [
            ("A", "Waterproofing specification", "Provide two membrane layers, each 4 mm thick."),
            (
                "B",
                "Roof detail",
                "Provide one 3 mm membrane layer for the same roof; no instruction resolves this discrepancy.",
            ),
        ],
        [
            ("spec_thickness", "8", "mm", ["A"]),
            ("detail_thickness", "3", "mm", ["B"]),
            ("difference", "5", "mm", ["A", "B"]),
        ],
        [("specification_conflict", ["A", "B"])],
        ("product", {"quantity": "2", "factor": "4"}, {}, {"product": "8.00", "unit": "1"}),
    ),
    _case(
        "spec-04",
        "specification_conflict",
        "Equivalent handrail units",
        "Normalize the two handrail heights and decide whether their dimensions conflict.",
        [
            ("A", "Handrail schedule", "Required height: 1100 mm."),
            ("B", "Architectural note", "Required height for the same handrail: 1.1 m."),
        ],
        [("normalized_height", "1100", "mm", ["A", "B"])],
        [("equivalent_units", ["A", "B"])],
        ("product", {"quantity": "1.1", "factor": "1000"}, {}, {"product": "1100.00", "unit": "1"}),
        forbidden=["specification_conflict"],
    ),
    _case(
        "rev-01",
        "drawing_revision",
        "Revised slab size",
        "Use revision B for current slab concrete and report the change from revision A.",
        [
            ("A", "Slab revision A", "Superseded revision A: area 100 m2, thickness 0.20 m."),
            (
                "B",
                "Slab revision B",
                "Current approved revision B replaces A: area 120 m2, thickness 0.25 m.",
            ),
        ],
        [("current_quantity", "30", "m3", ["B"]), ("quantity_change", "10", "m3", ["A", "B"])],
        [("superseded_revision", ["A", "B"])],
        (
            "product",
            {"quantity": "120", "factor": "0.25"},
            {"quantity": "m2", "factor": "m"},
            {"product": "30.00", "unit": "m3"},
        ),
    ),
    _case(
        "rev-02",
        "drawing_revision",
        "Additional footings",
        "Calculate current footing concrete and the added quantity introduced by revision C.",
        [
            (
                "A",
                "Footing revision B",
                "Revision B contains 8 footings. Every footing is 1.5 m by 1.5 m by 0.4 m.",
            ),
            (
                "B",
                "Footing revision C",
                "Current revision C supersedes B and contains 10 identical footings. Footing dimensions remain unchanged.",
            ),
        ],
        [("current_quantity", "9", "m3", ["A", "B"]), ("quantity_change", "1.8", "m3", ["A", "B"])],
        [("superseded_revision", ["A", "B"])],
        ("product", {"quantity": "10", "factor": "0.9"}, {}, {"product": "9.00", "unit": "1"}),
    ),
    _case(
        "rev-03",
        "drawing_revision",
        "Pipe rerouting cost",
        "Calculate additional pipe length and cost introduced by the current rerouting notice.",
        [
            ("A", "Old route schedule", "Superseded route length: 60 m."),
            (
                "B",
                "Current rerouting notice",
                "Current route length: 72 m. Approved comparison rate: EGP 400/m; unchanged between revisions.",
            ),
        ],
        [("added_length", "12", "m", ["A", "B"]), ("added_cost", "4800", "EGP", ["A", "B"])],
        [("superseded_revision", ["A", "B"])],
        ("product", {"quantity": "12", "factor": "400"}, {}, {"product": "4800.00", "unit": "1"}),
    ),
    _case(
        "rev-04",
        "drawing_revision",
        "Changed window mix",
        "Compare revision A and B window counts and amounts; do not assume unchanged total count means unchanged cost.",
        [
            (
                "A",
                "Window revision A",
                "Revision A: 12 W1 windows at EGP 1200 each; 4 W2 windows at EGP 1500 each.",
            ),
            (
                "B",
                "Window revision B",
                "Current revision B replaces A: 10 W1 windows and 6 W2 windows. Rates remain W1 EGP 1200 and W2 EGP 1500.",
            ),
        ],
        [
            ("current_count", "16", "each", ["B"]),
            ("current_amount", "21000", "EGP", ["B"]),
            ("cost_change", "600", "EGP", ["A", "B"]),
        ],
        [("mix_changed", ["A", "B"])],
        ("sum", {"values": ["12000", "9000"]}, {}, {"sum": "21000.00", "unit": "1"}),
    ),
    _case(
        "market-01",
        "market_observation",
        "Landed steel observation",
        "Normalize the supplied synthetic price observation to EGP per tonne including freight, excluding tax. Do not search for a new price.",
        [
            (
                "A",
                "Synthetic steel quote",
                "Observed 1 September 2026: USD 650/t, excluding taxes and freight. Freight is EGP 1500/t.",
            ),
            (
                "B",
                "Comparison exchange rate",
                "Use the supplied comparison basis USD 1 = EGP 50. This is synthetic, not a current financial quote.",
            ),
        ],
        [("landed_rate", "34000", "EGP/t", ["A", "B"])],
        [("currency_converted", ["A", "B"]), ("tax_excluded", ["A"])],
        ("sum", {"values": ["32500", "1500"]}, {}, {"sum": "34000.00", "unit": "1"}),
    ),
    _case(
        "market-02",
        "market_observation",
        "Remove included tax",
        "Normalize the observation to a price excluding VAT, using the explicitly supplied synthetic tax factor.",
        [
            (
                "A",
                "Synthetic concrete observation",
                "Concrete price EGP 2000/m3 includes exactly 14% VAT. Net price = gross / 1.14. Round to two decimals; no other charges.",
            )
        ],
        [("net_rate", "1754.39", "EGP/m3", ["A"]), ("included_tax", "245.61", "EGP/m3", ["A"])],
        [("tax_normalized", ["A"])],
        (
            "difference",
            {"left": "2000", "right": "1754.39"},
            {},
            {"difference": "245.61", "unit": "1"},
        ),
    ),
    _case(
        "market-03",
        "market_observation",
        "Exclude expired observation",
        "Select the lowest still-valid observed price as of 12 September 2026. Do not treat an expired quote as current.",
        [
            (
                "A",
                "Synthetic observation register",
                "All prices EGP/m3 excluding tax: Quote A 1600 valid through 1 September 2026; Quote B 1850 valid through 30 September 2026; Quote C 1900 valid through 20 September 2026. Assessment date: 12 September 2026.",
            )
        ],
        [
            ("valid_lowest_rate", "1850", "EGP/m3", ["A"]),
            ("difference_from_expired", "250", "EGP/m3", ["A"]),
        ],
        [("expired_quote_excluded", ["A"])],
        (
            "difference",
            {"left": "1850", "right": "1600"},
            {},
            {"difference": "250.00", "unit": "1"},
        ),
    ),
    _case(
        "market-04",
        "market_observation",
        "Bag-to-tonne normalization",
        "Convert the supplied cement bag price to a tonne rate on the same tax and delivery basis.",
        [
            (
                "A",
                "Synthetic cement quote",
                "Cement: EGP 400 per 50 kg bag. Exactly 1000 kg per tonne. Delivery included; VAT excluded.",
            )
        ],
        [("normalized_rate", "8000", "EGP/t", ["A"])],
        [("unit_normalized", ["A"])],
        ("product", {"quantity": "20", "factor": "400"}, {}, {"product": "8000.00", "unit": "1"}),
    ),
    _case(
        "supplier-01",
        "supplier_comparison",
        "Compare delivered amounts",
        "Compare two offers for 100 identical units on delivered, tax-excluded amount.",
        [
            (
                "A",
                "Supplier A synthetic offer",
                "100 units at EGP 90 each, plus EGP 1000 delivery. Excludes tax.",
            ),
            (
                "B",
                "Supplier B synthetic offer",
                "100 units at EGP 95 each, delivery included. Excludes tax. Both offers meet the same specification.",
            ),
        ],
        [
            ("supplier_a_total", "10000", "EGP", ["A"]),
            ("supplier_b_total", "9500", "EGP", ["B"]),
            ("saving", "500", "EGP", ["A", "B"]),
        ],
        [("supplier_b_preferred", ["A", "B"])],
        (
            "difference",
            {"left": "10000", "right": "9500"},
            {},
            {"difference": "500.00", "unit": "1"},
        ),
    ),
    _case(
        "supplier-02",
        "supplier_comparison",
        "Delivery-constrained choice",
        "Choose the compliant offer for 20 tonnes due within 10 days; report its total and advance payment.",
        [
            (
                "A",
                "Supplier A synthetic offer",
                "20 t at EGP 31000/t. Delivery in 30 days; 100% advance.",
            ),
            (
                "B",
                "Supplier B synthetic offer",
                "20 t at EGP 32000/t. Delivery in 7 days; 30% advance. Same specification, tax and freight basis as A. Required delivery: no more than 10 days.",
            ),
        ],
        [("selected_total", "640000", "EGP", ["B"]), ("advance_payment", "192000", "EGP", ["B"])],
        [("delivery_constraint", ["A", "B"]), ("supplier_b_preferred", ["A", "B"])],
        (
            "product",
            {"quantity": "640000", "factor": "0.30"},
            {},
            {"product": "192000.00", "unit": "1"},
        ),
    ),
    _case(
        "supplier-03",
        "supplier_comparison",
        "Tax-normalized selection",
        "Compare net amounts for 20 identical items using the supplied 14% VAT basis.",
        [
            (
                "A",
                "Supplier A synthetic offer",
                "EGP 1140 per item including 14% VAT; 20 items. Delivery included.",
            ),
            (
                "B",
                "Supplier B synthetic offer",
                "EGP 1050 per item excluding VAT; 20 items. Delivery included. Compare excluding VAT.",
            ),
        ],
        [
            ("supplier_a_net", "20000", "EGP", ["A"]),
            ("supplier_b_net", "21000", "EGP", ["B"]),
            ("saving", "1000", "EGP", ["A", "B"]),
        ],
        [("tax_normalized", ["A", "B"]), ("supplier_a_preferred", ["A", "B"])],
        (
            "difference",
            {"left": "21000", "right": "20000"},
            {},
            {"difference": "1000.00", "unit": "1"},
        ),
    ),
    _case(
        "supplier-04",
        "supplier_comparison",
        "Planted quote arithmetic error",
        "Check the supplier's multiplication and show the stated amount's overstatement.",
        [
            (
                "A",
                "Synthetic supplier quotation",
                "100 units at EGP 25/unit. Supplier states line amount EGP 2800. No tax or extra charges apply.",
            )
        ],
        [("correct_amount", "2500", "EGP", ["A"]), ("overstatement", "300", "EGP", ["A"])],
        [("quotation_arithmetic_error", ["A"])],
        ("product", {"quantity": "100", "factor": "25"}, {}, {"product": "2500.00", "unit": "1"}),
    ),
    _case(
        "integrated-01",
        "integrated_work",
        "Concrete quantity and waste cost",
        "Compute ordered quantity and cost including the specified waste allowance, independently check the calculation, and save a cited table.",
        [
            (
                "A",
                "Synthetic concrete takeoff",
                "Net concrete quantity 20 m3. Add exactly 5% waste to the ordered quantity.",
            ),
            (
                "B",
                "Synthetic rate",
                "Concrete supply rate EGP 2500/m3, excluding VAT. Apply it to ordered quantity.",
            ),
        ],
        [("ordered_quantity", "21", "m3", ["A"]), ("amount", "52500", "EGP", ["A", "B"])],
        [("waste_allowance", ["A"])],
        ("product", {"quantity": "21", "factor": "2500"}, {}, {"product": "52500.00", "unit": "1"}),
        check=True,
    ),
    _case(
        "integrated-02",
        "integrated_work",
        "Excavation and loose disposal",
        "Reconcile excavation with the BOQ, convert to loose disposal volume, price it, independently check arithmetic and save a cited table.",
        [
            (
                "A",
                "Synthetic excavation dimensions",
                "Excavation 10 m by 8 m by 2 m; no side slopes.",
            ),
            (
                "B",
                "BOQ and disposal basis",
                "BOQ excavation 150 m3. Loose disposal factor 1.20 times calculated excavation. Disposal EGP 80 per loose m3.",
            ),
        ],
        [
            ("excavation", "160", "m3", ["A"]),
            ("difference", "10", "m3", ["A", "B"]),
            ("loose_volume", "192", "m3", ["A", "B"]),
            ("disposal_cost", "15360", "EGP", ["A", "B"]),
        ],
        [("quantity_discrepancy", ["A", "B"])],
        ("product", {"quantity": "192", "factor": "80"}, {}, {"product": "15360.00", "unit": "1"}),
        check=True,
    ),
    _case(
        "integrated-03",
        "integrated_work",
        "Independent planted-error review",
        "Independently recompute the worksheet, identify the planted error, check the calculation receipt, and preserve the original figure as disputed.",
        [
            (
                "A",
                "Synthetic worksheet",
                "Area 100 m2; rate EGP 75/m2. Worksheet amount is stated as EGP 7000. There are no adjustments or deductions.",
            )
        ],
        [("correct_amount", "7500", "EGP", ["A"]), ("understatement", "500", "EGP", ["A"])],
        [("planted_error_found", ["A"])],
        ("product", {"quantity": "100", "factor": "75"}, {}, {"product": "7500.00", "unit": "1"}),
        check=True,
    ),
    _case(
        "integrated-04",
        "integrated_work",
        "Delivery versus budget tradeoff",
        "Choose the offer meeting the 10-day limit, compute landed cost and budget difference, independently check it and save the cited tradeoff.",
        [
            (
                "A",
                "Supplier A synthetic offer",
                "8 t at EGP 41000/t plus EGP 5000 freight; delivery in 9 days.",
            ),
            (
                "B",
                "Supplier B synthetic offer",
                "8 t at EGP 40000/t plus EGP 10000 freight; delivery in 14 days.",
            ),
            (
                "C",
                "Synthetic constraints",
                "Delivery deadline 10 days. Budget EGP 330000 excluding tax. Both offers meet the technical specification and exclude tax.",
            ),
        ],
        [("selected_cost", "333000", "EGP", ["A"]), ("budget_overrun", "3000", "EGP", ["A", "C"])],
        [
            ("delivery_constraint", ["A", "B", "C"]),
            ("budget_overrun", ["A", "C"]),
            ("supplier_a_preferred", ["A", "B", "C"]),
        ],
        ("sum", {"values": ["328000", "5000"]}, {}, {"sum": "333000.00", "unit": "1"}),
        check=True,
    ),
]
