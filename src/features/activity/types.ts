import type { Schema } from "../../api";
export type RunActivity = Schema<"RunActivity">;
export type RunActivityPage = Schema<"RunActivityPage">;
export type RunActivityDetail = Schema<"RunActivityDetail">;
export type ActivityFilters = {
  q: string;
  actor_id: string;
  category: string;
  errors_only: boolean;
};
export const emptyFilters: ActivityFilters = {
  q: "",
  actor_id: "",
  category: "",
  errors_only: false,
};
