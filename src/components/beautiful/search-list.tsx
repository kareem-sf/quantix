import { useState, type ReactNode } from "react";
import { Search, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { GlideMenu } from "./glide-menu";

export type SearchItem = {
  id: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
};

/**
 * A search field with live-filtered results and an empty state. Adapted from
 * Beautiful UI's Search. Filtering is local; pass `onQueryChange` to search a
 * backend instead and feed the results back through `items`.
 */
export function SearchList({
  items,
  onPick,
  placeholder,
  label,
  onQueryChange,
  filterLocally = true,
  limit,
  className,
}: {
  items: SearchItem[];
  onPick: (item: SearchItem) => void;
  placeholder: string;
  label: string;
  onQueryChange?: (query: string) => void;
  filterLocally?: boolean;
  limit?: number;
  className?: string;
}) {
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const matches =
    filterLocally && needle
      ? items.filter((item) =>
          `${item.label} ${item.hint ?? ""}`.toLowerCase().includes(needle),
        )
      : items;
  const results = limit ? matches.slice(0, limit) : matches;
  const change = (value: string) => {
    setQuery(value);
    onQueryChange?.(value);
  };

  return (
    <div
      className={cn(
        "w-full overflow-hidden rounded-lg bg-card ring-1 ring-border",
        className,
      )}
    >
      <div className="flex h-10 items-center gap-2 border-b px-3">
        <Search
          aria-hidden
          className="size-3.5 shrink-0 text-muted-foreground"
        />
        <input
          value={query}
          onChange={(event) => change(event.target.value)}
          placeholder={placeholder}
          aria-label={label}
          className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
        {query ? (
          <button
            type="button"
            aria-label="Clear search"
            onClick={() => change("")}
            className="bui-fade-in flex size-6 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <X aria-hidden className="size-3" />
          </button>
        ) : null}
      </div>
      {needle && !results.length ? (
        <div className="bui-fade-in flex flex-col items-center justify-center gap-1 px-4 py-8">
          <span className="mb-1.5 flex size-8 items-center justify-center rounded-md bg-muted text-muted-foreground">
            <Search aria-hidden className="size-4" />
          </span>
          <span className="text-sm font-medium">No results</span>
          <span className="text-xs text-muted-foreground">
            Try different words.
          </span>
        </div>
      ) : (
        <div className="p-1">
          <GlideMenu className="flex flex-col gap-px">
            {results.map((item) => (
              <button
                key={item.id}
                type="button"
                data-menu-row
                onClick={() => onPick(item)}
                className="relative z-10 flex min-h-8 w-full items-center gap-2 rounded-md px-2 py-1 text-start text-sm"
              >
                {item.icon ? (
                  <span className="shrink-0 text-muted-foreground">
                    {item.icon}
                  </span>
                ) : null}
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.hint ? (
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {item.hint}
                  </span>
                ) : null}
              </button>
            ))}
          </GlideMenu>
        </div>
      )}
    </div>
  );
}
