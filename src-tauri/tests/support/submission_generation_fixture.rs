pub(crate) fn candidate(
    data_view: &serde_json::Value,
    scenario: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let evidence = data_view
        .pointer("/evidence/0/handle")
        .cloned()
        .ok_or("Submission Generation Evidence handle")?;
    let field = |name: &str, value: &str| {
        serde_json::json!({
            "name": name,
            "value": value,
            "basis": {"kind": "evidence", "evidence": [evidence.clone()]},
            "original_expression": null,
            "normalized_value": null,
            "timezone": null,
            "uncertainty": null
        })
    };
    let definitions = [
        (
            "mandatory_submission_requirement",
            "mandatory_requirement",
            "Mandatory submission requirement / متطلب تقديم إلزامي",
            "technical",
            "technical_submission",
            "01-Technical/Technical-Submission.docx",
            "English / العربية",
            "docx",
        ),
        (
            "technical_deliverable",
            "deliverable",
            "Submit the exact technical method statement.",
            "technical",
            "technical_submission",
            "01-Technical/Technical-Submission.docx",
            "English / العربية",
            "docx",
        ),
        (
            "controlled_addendum_instruction",
            "addendum_instruction",
            "Apply Addendum 01 without changing its meaning.",
            "technical",
            "technical_submission",
            "01-Technical/Technical-Submission.docx",
            "English / العربية",
            "docx",
        ),
        (
            "signature_instruction",
            "signature",
            "The Tendering Manager signature is required.",
            "forms",
            "forms_submission",
            "03-Forms/\u{0646}\u{0645}\u{0648}\u{0630}\u{062c}-Signature.docx",
            "English",
            "docx",
        ),
        (
            "verified_form_field",
            "form_field",
            "Legal entity name / اسم الكيان القانوني",
            "forms",
            "forms_submission",
            "03-Forms/Bilingual-Form-Fields.docx",
            "English / العربية",
            "docx",
        ),
        (
            "commercial_execution_requirement",
            "execution_requirement",
            "Display only the exact approved Final Price and calculation provenance.",
            "commercial",
            "commercial_submission",
            "02-Commercial/Commercial-Offer.xlsx",
            "English / العربية",
            "xlsx",
        ),
        (
            "unchanged_required_file",
            "required_file",
            "Include the supplied form byte-for-byte unchanged.",
            "forms",
            "forms_submission",
            "03-Forms/Original-Supplied-Form.pdf",
            "Arabic",
            "unchanged_source",
        ),
    ];
    let mut records = definitions
        .into_iter()
        .map(
            |(stable_key, kind, text, section, envelope, path, language, authoring_mode)| {
                serde_json::json!({
                    "stable_key": stable_key,
                    "kind": "requirement",
                    "title": text,
                    "generation_instruction": {
                        "kind": kind,
                        "mandatory": true,
                        "section_key": section,
                        "package_path": path,
                        "envelope_key": envelope,
                        "language": language,
                        "authoring_mode": authoring_mode,
                        "requested_authoring_format": null,
                        "evidence": [evidence.clone()]
                    },
                    "fields": [
                        field("submission_text", text)
                    ],
                    "contradictions": []
                })
            },
        )
        .collect::<Vec<_>>();
    records[1]["fields"]
        .as_array_mut()
        .ok_or("technical deliverable fields")?
        .extend([
            field("delivery_date", "1 June 2026"),
            field("submission_quantity", "2 signed originals"),
            field("delivery_schedule", "Mobilization and handover milestones"),
            field("approved_qualification", "Offer validity is 90 days"),
            field("approved_exclusion", "No unverified alternative scope"),
        ]);
    if scenario == "record-extraction-submission-generation-unsupported" {
        records[0]["generation_instruction"]["authoring_mode"] = serde_json::json!("unsupported");
        records[0]["generation_instruction"]["requested_authoring_format"] =
            serde_json::json!("fillable_pdf");
    }
    if scenario == "record-extraction-submission-generation-all-unsupported" {
        for record in &mut records {
            record["generation_instruction"]["authoring_mode"] = serde_json::json!("unsupported");
            record["generation_instruction"]["requested_authoring_format"] =
                serde_json::json!("external_portal_template");
        }
    }
    if scenario == "record-extraction-submission-generation-path-collision" {
        records[0]["generation_instruction"]["package_path"] =
            serde_json::json!("01-Technical/Ｍaße.docx");
        records[1]["generation_instruction"]["package_path"] =
            serde_json::json!("01-technical/masse.docx");
    }
    if scenario == "record-extraction-submission-generation-missing" {
        let exact_sources = data_view
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .ok_or("Submission Generation Evidence inventory")?
            .iter()
            .filter_map(|entry| entry.get("handle").cloned())
            .collect::<Vec<_>>();
        records[6]["generation_instruction"]["evidence"] = serde_json::Value::Array(exact_sources);
    }
    if scenario == "record-extraction-submission-generation-empty-material" {
        records[0]["fields"][0]["value"] = serde_json::Value::Null;
        records[0]["fields"][0]["normalized_value"] = serde_json::Value::Null;
    }
    let mut candidate = super::coordinated_record_extraction_candidate(data_view, None)?;
    candidate["records"]
        .as_array_mut()
        .ok_or("base Tender Record candidate records")?
        .extend(records);
    Ok(candidate)
}
