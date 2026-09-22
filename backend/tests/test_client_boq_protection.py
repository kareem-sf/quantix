"""A client workbook counts as protected only when Excel would enforce it."""

import zipfile

from openpyxl import Workbook

from quantix.client_boq import SHEET_NS, _locked, _xml


def _protection(path, member, tag):
    with zipfile.ZipFile(path) as archive:
        return _xml(archive.read(member)).find(f"{{{SHEET_NS}}}{tag}")


def _saved(tmp_path, configure=None):
    workbook = Workbook()
    workbook.active.title = "BOQ"
    if configure:
        configure(workbook)
    path = tmp_path / "BOQ.xlsx"
    workbook.save(path)
    return path


def test_an_empty_protection_element_written_by_tools_is_not_a_lock(tmp_path):
    path = _saved(tmp_path)
    workbook = _protection(path, "xl/workbook.xml", "workbookProtection")
    assert workbook is not None  # openpyxl always writes it
    assert not _locked(workbook, ("lockStructure", "lockWindows", "lockRevision"))
    assert not _locked(_protection(path, "xl/worksheets/sheet1.xml", "sheetProtection"), ("sheet",))


def test_real_workbook_and_sheet_locks_are_still_refused(tmp_path):
    def lock(workbook):
        workbook.security.lockStructure = True
        workbook.active.protection.sheet = True

    path = _saved(tmp_path, lock)
    assert _locked(
        _protection(path, "xl/workbook.xml", "workbookProtection"),
        ("lockStructure", "lockWindows", "lockRevision"),
    )
    assert _locked(_protection(path, "xl/worksheets/sheet1.xml", "sheetProtection"), ("sheet",))
