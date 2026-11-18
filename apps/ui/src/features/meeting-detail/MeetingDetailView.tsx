import { useState } from "react";
import type { MeetingStatus, MeetingSummary } from "../../ipc/types";
import { formatDateTime, formatDuration } from "../../shared/format";
import { useLanguage, useT } from "../../shared/i18n";
import type { Async } from "../../shared/useAsync";
import {
  AsyncContent,
  Banner,
  Button,
  Dialog,
  EmptyState,
  ErrorState,
  LoadingState,
  NotAvailable,
  Page,
  Section,
} from "../../shared/ui";
import { MeetingStatusBadge } from "../meetings";

export interface MeetingDetailViewProps {
  /** The meeting, or null when the id is not in the library. */
  meeting: Async<MeetingSummary | null>;
  onBack: () => void;
  onRetry: () => void;
}

/** Summary region: generating, ready and failed states. Interrupted shows ready content with a warning. */
export function SummarySection({ status }: { status: MeetingStatus }) {
  const t = useT();
  return (
    <Section title={t("detail.summary")}>
      {status === "generating" ? <LoadingState label={t("detail.generating")} /> : null}
      {status === "failed" ? (
        <div className="stack">
          <ErrorState message={t("detail.summaryFailed")} />
          {/* No command retries a summary yet, so the button stays off and says why. */}
          <div className="row">
            <Button disabled aria-describedby="summary-retry-note">
              {t("common.retry")}
            </Button>
            <span id="summary-retry-note" className="muted">
              {t("detail.retryUnavailable")}
            </span>
          </div>
        </div>
      ) : null}
      {status === "ready" || status === "interrupted" ? (
        <div className="stack">
          {status === "interrupted" ? (
            <Banner kind="warning">{t("detail.interrupted")}</Banner>
          ) : null}
          <h3>{t("detail.keyPoints")}</h3>
          <NotAvailable>{t("detail.keyPointsBody")}</NotAvailable>
          <h3>{t("detail.actionItems")}</h3>
          <NotAvailable>{t("detail.actionItemsBody")}</NotAvailable>
        </div>
      ) : null}
    </Section>
  );
}

// D-46: the text of "detail.deleteBody" says what deleting a meeting removes, and what stays.
function DeleteMeetingDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  return (
    <Dialog title={t("detail.deleteTitle")} onClose={onClose}>
      <div className="stack">
        <p>{t("detail.deleteBody")}</p>
        {/* No command deletes a meeting yet, so the button stays off and says why. */}
        <div className="row">
          <Button disabled aria-describedby="delete-meeting-note">
            {t("detail.deleteConfirm")}
          </Button>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
        </div>
        <p id="delete-meeting-note" className="muted">
          {t("detail.deleteUnavailable")}
        </p>
      </div>
    </Dialog>
  );
}

function MeetingBody({ meeting, onBack }: { meeting: MeetingSummary; onBack: () => void }) {
  const [deleting, setDeleting] = useState(false);
  const t = useT();
  const { language } = useLanguage();
  return (
    <>
      <Section title={t("detail.overview")}>
        <p>
          {formatDateTime(meeting.startedAtMs, language)} · {formatDuration(meeting.durationSec)}{" "}
          <MeetingStatusBadge status={meeting.status} />
        </p>
      </Section>
      <SummarySection status={meeting.status} />
      <Section title={t("detail.transcript")}>
        <NotAvailable>{t("detail.transcriptBody")}</NotAvailable>
      </Section>
      <div className="row">
        <Button onClick={onBack}>{t("detail.back")}</Button>
        <Button onClick={() => setDeleting(true)}>{t("detail.delete")}</Button>
      </div>
      {deleting ? <DeleteMeetingDialog onClose={() => setDeleting(false)} /> : null}
    </>
  );
}

export function MeetingDetailView({ meeting, onBack, onRetry }: MeetingDetailViewProps) {
  const t = useT();
  return (
    <Page title={t("detail.title")}>
      <AsyncContent
        state={meeting}
        onRetry={onRetry}
        loadingLabel={t("detail.loading")}
        isEmpty={(m) => m === null}
        empty={
          <EmptyState
            title={t("detail.notFoundTitle")}
            description={t("detail.notFoundBody")}
            action={<Button onClick={onBack}>{t("detail.back")}</Button>}
          />
        }
      >
        {(m) => (m ? <MeetingBody meeting={m} onBack={onBack} /> : null)}
      </AsyncContent>
    </Page>
  );
}
