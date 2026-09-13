import {
  Archive,
  Copy,
  Download,
  History,
  Pencil,
  UserRoundPlus,
} from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { AgentDefinition } from "./agentLibraryTypes";

export function AgentDefinitionCard({
  definition,
  versions,
  historyPending,
  onUse,
  onEdit,
  onDuplicate,
  onHistory,
  onExport,
  onRetire,
}: {
  definition: AgentDefinition;
  versions?: AgentDefinition[];
  historyPending: boolean;
  onUse: (definition: AgentDefinition) => void;
  onEdit: (definition: AgentDefinition) => void;
  onDuplicate: (definition: AgentDefinition) => void;
  onHistory: (definition: AgentDefinition) => void;
  onExport: (definition: AgentDefinition) => void;
  onRetire: (definition: AgentDefinition) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{definition.profile.display_name}</CardTitle>
        <CardDescription>
          {definition.profile.title} · {definition.profile.role}
        </CardDescription>
        <CardAction>
          <Badge variant="secondary">Version {definition.version}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <p>{definition.profile.persona}</p>
        <div className="flex flex-wrap gap-1.5">
          {definition.profile.specialisms.map((item) => (
            <Badge key={item} variant="outline">
              {item}
            </Badge>
          ))}
        </div>
        <details>
          <summary className="cursor-pointer text-sm font-medium">
            Profile and settings
          </summary>
          <dl className="mt-3 grid gap-2 text-sm sm:grid-cols-2">
            <Fact
              label="Requested tools"
              value={definition.profile.requested_tool_ids.join(", ") || "None"}
            />
            <Fact
              label="Thinking"
              value={definition.generation_settings.reasoning || "AI default"}
            />
            <Fact
              label="Output limit"
              value={`${definition.generation_settings.max_output_tokens} tokens`}
            />
          </dl>
        </details>
        {versions ? (
          <section
            aria-label={`${definition.profile.display_name} version history`}
          >
            <h3 className="text-sm font-medium">Saved versions</h3>
            <ul className="mt-2 flex flex-col gap-2 text-sm text-muted-foreground">
              {versions.map((item) => (
                <li
                  key={item.version}
                  className="flex flex-wrap items-center justify-between gap-2"
                >
                  <span>
                    {item.profile.display_name} · version {item.version}
                  </span>
                  <div className="flex gap-1">
                    <Button
                      type="button"
                      size="xs"
                      variant="ghost"
                      onClick={() => onUse(item)}
                    >
                      Use
                    </Button>
                    <Button
                      type="button"
                      size="xs"
                      variant="ghost"
                      onClick={() => onDuplicate(item)}
                    >
                      Duplicate
                    </Button>
                    <Button
                      type="button"
                      size="xs"
                      variant="ghost"
                      onClick={() => onExport(item)}
                    >
                      Export
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <Button
          type="button"
          size="sm"
          onClick={() => onUse(definition)}
          aria-label={`Use ${definition.profile.display_name} for a Tender`}
        >
          <UserRoundPlus data-icon="inline-start" />
          Use for Tender
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => onEdit(definition)}
          aria-label={`Edit ${definition.profile.display_name}`}
        >
          <Pencil data-icon="inline-start" />
          Edit
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => onDuplicate(definition)}
          aria-label={`Duplicate ${definition.profile.display_name}`}
        >
          <Copy data-icon="inline-start" />
          Duplicate
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={historyPending}
          onClick={() => onHistory(definition)}
          aria-label="Show version history"
        >
          <History data-icon="inline-start" />
          {historyPending ? "Loading…" : "Versions"}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={() => onExport(definition)}
          aria-label={`Export ${definition.profile.display_name}`}
        >
          <Download data-icon="inline-start" />
          Export
        </Button>
        <AlertDialog>
          <AlertDialogTrigger
            render={
              <Button
                type="button"
                size="sm"
                variant="ghost"
                aria-label={`Retire ${definition.profile.display_name}`}
              />
            }
          >
            <Archive data-icon="inline-start" />
            Retire
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                Retire {definition.profile.display_name}?
              </AlertDialogTitle>
              <AlertDialogDescription>
                It will no longer be available for new Tender work. Existing
                Tender work keeps its saved version.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Keep professional</AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                onClick={() => onRetire(definition)}
              >
                Retire professional
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </CardFooter>
    </Card>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-muted-foreground">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
