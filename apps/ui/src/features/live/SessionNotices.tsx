import type { SessionState } from "../../ipc/meetingSession";
import { useT } from "../../shared/i18n";
import { Banner, Button, LoadingState } from "../../shared/ui";

/**
 * The session's banner, when it has one. With `onDismiss` the banner gets a
 * dismiss button; the small views (subtitle bar, compact panel) show it as is.
 */
export function SessionBanner({
  session,
  onDismiss,
}: {
  session: SessionState;
  onDismiss?: () => void;
}) {
  const t = useT();
  if (!session.banner) return null;
  return (
    <Banner
      kind={session.banner.kind}
      actions={
        onDismiss ? (
          <Button variant="ghost" aria-label={t("live.dismissMessage")} onClick={onDismiss}>
            ×
          </Button>
        ) : undefined
      }
    >
      {session.banner.message}
    </Banner>
  );
}

/** The loading line shown while a meeting starts; nothing in any other state. */
export function StartingNotice({ session }: { session: SessionState }) {
  const t = useT();
  return session.status === "starting" ? <LoadingState label={t("live.starting")} /> : null;
}
