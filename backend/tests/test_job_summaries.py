"""What each job did, counted from recorded facts for the Activity list."""

from quantix.job_summaries import JobSummaryService
from quantix.repository import Repository


def _tool(repo, run_id, phase, fact):
    repo.event(run_id, "tool_" + phase, fact.get("line", ""), {"phase": phase, "fact": fact})


def test_counts_only_finished_facts_and_distinct_documents(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic warehouse")
    run = repo.create_run(tender["id"], "manager", "review the package")
    quiet = repo.create_run(tender["id"], "manager", "who are you?")
    other = repo.create_tender("Another")
    elsewhere = repo.create_run(other["id"], "manager", "read")
    for subject in ["Spec.pdf", "Spec.pdf", "BOQ.xlsx"]:
        _tool(repo, run["id"], "completed", {"kind": "read", "line": "Read", "subject": subject})
    _tool(repo, run["id"], "started", {"kind": "read", "line": "Reading", "subject": "Draw.pdf"})
    _tool(repo, run["id"], "failed", {"kind": "search", "line": "Tried", "state": "failed"})
    _tool(repo, run["id"], "completed", {"kind": "search", "line": "Searched"})
    _tool(repo, run["id"], "completed", {"kind": "hire", "line": "Hired", "subject": "Layla"})
    _tool(repo, run["id"], "completed", {"kind": "assign", "line": "Asked", "subject": "Layla"})
    _tool(repo, run["id"], "completed", {"kind": "propose", "line": "Prepared"})
    _tool(repo, elsewhere["id"], "completed", {"kind": "read", "line": "Read", "subject": "X"})

    jobs = {job.run_id: job for job in JobSummaryService(repo).list(tender["id"]).jobs}

    assert set(jobs) == {run["id"]}
    job = jobs[run["id"]]
    assert job.documents == ["Spec.pdf", "BOQ.xlsx"]
    assert (job.searches, job.page_views, job.staff_hired, job.proposals) == (1, 0, 1, 1)
    assert job.colleagues_asked == ["Layla"]
    assert quiet["id"] not in jobs
