"""Explicit whole-working-day construction sequencing."""

from datetime import timedelta
from graphlib import CycleError, TopologicalSorter


def schedule_programme(programme):
    """Inclusive whole working days and zero-lag finish-to-start relationships."""
    activities = {activity.id: activity for activity in programme.activities}
    graph = {key: activity.predecessor_ids for key, activity in activities.items()}
    if any(
        predecessor not in activities
        for predecessors in graph.values()
        for predecessor in predecessors
    ):
        raise ValueError("Every predecessor must identify a supplied construction activity.")
    try:
        ordered = tuple(TopologicalSorter(graph).static_order())
    except CycleError as exc:
        raise ValueError("Construction activity predecessors contain a cycle.") from exc
    working, holidays = set(programme.working_week), set(programme.holidays)
    calendar, finish_indices, rows = [], {}, []
    cursor = programme.start_date

    def working_date(index):
        nonlocal cursor
        while len(calendar) <= index:
            if (cursor - programme.start_date).days > 36600:
                raise ValueError("The programme exceeds the supported 100-year calendar horizon.")
            if cursor.weekday() in working and cursor not in holidays:
                calendar.append(cursor)
            try:
                cursor += timedelta(days=1)
            except OverflowError as exc:
                raise ValueError("The programme exceeds the supported date range.") from exc
        return calendar[index].isoformat()

    for identifier in ordered:
        activity = activities[identifier]
        start = max(
            (finish_indices[predecessor] + 1 for predecessor in activity.predecessor_ids), default=0
        )
        finish_indices[identifier] = start + activity.duration_days - 1
        rows.append(
            activity.model_dump()
            | {
                "start_date": working_date(start),
                "finish_date": working_date(finish_indices[identifier]),
            }
        )
    return rows
