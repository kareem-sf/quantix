import { useGates } from "../estimate/queries";
import { useDecisions, type Decision } from "../office/queries";

export type Approval = { key: string; title: string; text: string; to: string };

/** What waits for the engineer on a tender: work the Tender Manager accepted, each kind one click from the screen
 * it is decided on, and the questions put to them. */
export function useNeedsYou(tenderId: string): { approvals: Approval[]; questions: Decision[] } {
  const gates = useGates(tenderId).data;
  const decisions = useDecisions(tenderId).data;
  const approvals = [
    gates?.facts && {
      key: "facts",
      title: `${gates.facts} tender ${gates.facts === 1 ? "fact" : "facts"} to approve`,
      text: "Method of measurement, currency or VAT, as the office read them",
      to: `/tenders/${tenderId}/estimate`,
    },
    gates?.takeoff && {
      key: "takeoff",
      title: `${gates.takeoff} takeoff ${gates.takeoff === 1 ? "mark" : "marks"} to check`,
      text: "Scales and measurements the team drew on the drawings",
      to: `/tenders/${tenderId}/takeoff`,
    },
    gates?.drawings && {
      key: "drawings",
      title: `${gates.drawings} ${gates.drawings === 1 ? "query or layer map" : "queries and layer maps"} to decide`,
      text: "Tender queries for the client, and what the drawings’ layers are",
      to: `/tenders/${tenderId}/queries`,
    },
    gates?.pricing && {
      key: "pricing",
      title: `${gates.pricing} ${gates.pricing === 1 ? "price" : "prices"} to approve`,
      text: "Rates and markups from the office, each with its build-up or source",
      to: `/tenders/${tenderId}/estimate?show=waiting`,
    },
    gates?.subcontract && {
      key: "subcontract",
      title: `${gates.subcontract} ${gates.subcontract === 1 ? "quote" : "quotes"} to choose`,
      text: "Levelled subcontract and supplier quotes with the office’s recommendation",
      to: `/tenders/${tenderId}/subcontract`,
    },
    gates?.enquiries && {
      key: "enquiries",
      title: `${gates.enquiries} ${gates.enquiries === 1 ? "enquiry" : "enquiries"} to send`,
      text: "Enquiries the office drafted, to send from your own mail",
      to: `/tenders/${tenderId}/subcontract`,
    },
    gates?.submission && {
      key: "submission",
      title: `${gates.submission} ${gates.submission === 1 ? "draft" : "drafts"} to review`,
      text: "Submission documents the office drafted from the tender’s requirements",
      to: `/tenders/${tenderId}/submission?show=review`,
    },
    gates?.boq && {
      key: "boq",
      title: `${gates.boq} BOQ ${gates.boq === 1 ? "item" : "items"} to approve`,
      text: "Entered by the office from the client’s BOQ, each with its page",
      to: `/tenders/${tenderId}/estimate?show=waiting`,
    },
  ].filter((a): a is Approval => Boolean(a));
  return { approvals, questions: (decisions ?? []).filter((d) => d.status === "waiting") };
}
