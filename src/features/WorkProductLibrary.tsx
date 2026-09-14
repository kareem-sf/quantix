import { useState } from "react";
import { ArrowLeft, ChevronRight } from "lucide-react";
import { tenderPath, useResource, type Schema } from "../api";
import { Empty, ErrorNotice, Loading } from "../components/common";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { WorkProductView } from "./WorkProductView";
import { useWorkProductPages } from "./useWorkProductPages";
import { useWorkProductRows } from "./useWorkProductRows";

type Summary = Schema<"WorkProductVersionSummary">;

export function WorkProductLibrary({
  tenderId,
  tenderRevision = 0,
  workRevision = "",
  onSource,
  focusedProductId,
  onBack,
}: {
  tenderId: string;
  tenderRevision?: number;
  workRevision?: string;
  onSource: (sourceId: string) => void;
  focusedProductId?: string;
  onBack?: () => void;
}) {
  if (focusedProductId)
    return (
      <FocusedWorkProduct
        key={`${tenderId}:${focusedProductId}`}
        tenderId={tenderId}
        productId={focusedProductId}
        refreshKey={`${tenderRevision}:${workRevision}`}
        onSource={onSource}
        onBack={onBack}
      />
    );
  return (
    <WorkProductList
      tenderId={tenderId}
      tenderRevision={tenderRevision}
      workRevision={workRevision}
      onSource={onSource}
    />
  );
}

function FocusedWorkProduct({
  tenderId,
  productId,
  refreshKey,
  onSource,
  onBack,
}: {
  tenderId: string;
  productId: string;
  refreshKey: string;
  onSource: (sourceId: string) => void;
  onBack?: () => void;
}) {
  const base = `${tenderPath(tenderId)}/work-products`;
  const current = useResource<Schema<"WorkProductPage">>(
    `${base}/${encodeURIComponent(productId)}/versions?offset=0&limit=1`,
  );
  const [showList, setShowList] = useState(false);
  if (showList)
    return (
      <WorkProductList
        tenderId={tenderId}
        onSource={onSource}
      />
    );
  if (current.isPending) return <Loading>Opening saved draft…</Loading>;
  if (current.error || !current.data?.items?.[0])
    return (
      <div>
        <ErrorNotice
          error={current.error ?? new Error("This saved draft is unavailable.")}
        />
        <Button onClick={() => void current.refetch()}>
          Retry saved draft
        </Button>
      </div>
    );
  return (
    <WorkProductDetail
      tenderId={tenderId}
      tenderRevision={refreshKey}
      base={base}
      initial={current.data.items[0]}
      onSource={onSource}
      onBack={onBack ?? (() => setShowList(true))}
    />
  );
}

function WorkProductList({
  tenderId,
  tenderRevision = 0,
  workRevision = "",
  onSource,
}: {
  tenderId: string;
  tenderRevision?: number;
  workRevision?: string;
  onSource: (sourceId: string) => void;
}) {
  const base = `${tenderPath(tenderId)}/work-products`;
  const refreshKey = `${tenderRevision}:${workRevision}`;
  const products = useWorkProductPages(base, refreshKey);
  const [selected, setSelected] = useState<Summary | null>(null);

  if (selected)
    return (
      <WorkProductDetail
        tenderId={tenderId}
        tenderRevision={refreshKey}
        base={base}
        initial={selected}
        onBack={() => setSelected(null)}
        onSource={onSource}
      />
    );
  return (
    <section className="flex flex-col gap-3" aria-label="Draft work products">
      <div>
        <h3 className="text-sm font-medium">Draft work products</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Inspect saved tables, charts and notes with their exact sources and
          version history.
        </p>
        {!products.error ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={products.loading}
            onClick={products.refresh}
          >
            Refresh saved drafts
          </Button>
        ) : null}
      </div>
      <ErrorNotice error={products.error} />
      {products.error ? (
        <Button
          type="button"
          variant="outline"
          disabled={products.loading}
          onClick={products.refresh}
        >
          Retry draft list
        </Button>
      ) : null}
      {products.loading && !products.items.length ? (
        <Loading>Loading draft work products…</Loading>
      ) : null}
      {products.items.map((item) => (
        <article className="rounded-lg border p-3" key={item.product_id}>
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <strong className="block text-sm" dir="auto">
                {item.title}
              </strong>
              <span className="text-xs text-muted-foreground">
                {kindLabel(item.kind)} · version {item.version} ·{" "}
                {item.row_count.toLocaleString()} rows
              </span>
            </div>
            <Badge
              variant={
                item.dependency_state === "needs_review"
                  ? "destructive"
                  : "secondary"
              }
            >
              {item.dependency_state === "needs_review"
                ? "Needs review"
                : "Current basis"}
            </Badge>
          </div>
          <Button
            type="button"
            variant="ghost"
            className="mt-2 h-8 w-full justify-between px-0 font-normal"
            aria-label={`Open ${item.title}`}
            onClick={() => setSelected(item)}
          >
            Inspect draft and history
            <ChevronRight />
          </Button>
        </article>
      ))}
      {!products.loading && !products.error && !products.items.length ? (
        <Empty title="No draft work products yet">
          Saved staff tables, charts and notes will appear here for review.
        </Empty>
      ) : null}
      {products.nextOffset != null ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={products.loading}
          onClick={() => void products.loadMore()}
        >
          {products.loading
            ? "Loading older products…"
            : "Load older draft work products"}
        </Button>
      ) : null}
      {products.items.length ? (
        <p className="text-xs text-muted-foreground">
          Showing {products.items.length} of {products.total} products.
        </p>
      ) : null}
    </section>
  );
}

function WorkProductDetail({
  tenderId,
  tenderRevision,
  base,
  initial,
  onBack,
  onSource,
}: {
  tenderId: string;
  tenderRevision: number | string;
  base: string;
  initial: Summary;
  onBack: () => void;
  onSource: (sourceId: string) => void;
}) {
  const [version, setVersion] = useState(initial.version);
  const productBase = `${base}/${encodeURIComponent(initial.product_id)}`;
  const history = useWorkProductPages(
    `${productBase}/versions`,
    tenderRevision,
  );
  const detail = useResource<Schema<"WorkProductVersion">>(
    `${productBase}/versions/${version}?include_rows=false`,
  );
  const rows = useWorkProductRows(`${productBase}/versions/${version}/rows`);
  return (
    <section className="flex flex-col gap-4" aria-label="Work product detail">
      <div className="flex items-start gap-2">
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="Back to draft work products"
          onClick={onBack}
        >
          <ArrowLeft />
        </Button>
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-medium" dir="auto">
            {detail.data?.title ?? initial.title}
          </h3>
          <p className="text-xs text-muted-foreground">
            Draft version {version} · Saved
          </p>
        </div>
        <Badge
          variant={
            detail.data?.dependency_state === "needs_review"
              ? "destructive"
              : "secondary"
          }
        >
          {detail.data?.dependency_state === "needs_review"
            ? "Needs review"
            : "Current basis"}
        </Badge>
      </div>
      {(detail.data?.review_reasons ?? []).map((reason) => (
        <p role="alert" className="text-sm text-destructive" key={reason}>
          {reason.replaceAll("_", " ")}
        </p>
      ))}
      <ErrorNotice error={history.error || detail.error || rows.error} />
      <Button
        type="button"
        variant={
          history.error || detail.error || rows.error ? "outline" : "ghost"
        }
        className="self-start"
        disabled={history.loading || rows.loading || detail.isFetching}
        onClick={() => {
          history.refresh();
          rows.refresh();
          void detail.refetch();
        }}
      >
        {history.error || detail.error || rows.error
          ? "Retry saved draft"
          : "Refresh draft and history"}
      </Button>
      {history.loading ||
      detail.isPending ||
      (rows.loading && !rows.items.length) ? (
        <Loading>Loading the saved draft…</Loading>
      ) : null}
      {detail.data &&
      (!rows.loading || rows.items.length > 0) &&
      (!rows.error || rows.items.length > 0) ? (
        <WorkProductView
          tenderId={tenderId}
          product={detail.data}
          rows={{ items: rows.items, total: rows.total, missing: rows.missing }}
          onSource={onSource}
        />
      ) : null}
      {rows.items.length < rows.total ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={rows.loading}
          onClick={() => void rows.loadMore()}
        >
          {rows.loading ? "Loading more rows…" : "Load more rows"}
        </Button>
      ) : null}
      {history.items.length ? (
        <fieldset className="space-y-2" aria-label="Version history">
          <legend className="text-xs font-medium">Version history</legend>
          {history.items.map((item) => (
            <Button
              key={item.id}
              type="button"
              variant={item.version === version ? "secondary" : "ghost"}
              size="sm"
              className="w-full justify-start font-normal"
              aria-label={`Version ${item.version} · ${item.title}`}
              onClick={() => setVersion(item.version)}
            >
              Version {item.version} · {item.title}
            </Button>
          ))}
          {history.nextOffset != null ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={history.loading}
              onClick={() => void history.loadMore()}
            >
              {history.loading
                ? "Loading older versions…"
                : "Load older versions"}
            </Button>
          ) : null}
        </fieldset>
      ) : null}
    </section>
  );
}

function kindLabel(value: Summary["kind"]) {
  return value.replaceAll("_", " ");
}
