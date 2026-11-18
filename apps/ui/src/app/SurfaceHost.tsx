import { CompactPanelScreen } from "../features/compact-panel";
import { QuickActionsScreen } from "../features/quick-actions";
import { SubtitleBarScreen } from "../features/subtitle-bar";
import { useT } from "../shared/i18n";
import { Toast } from "../shared/ui";
import { useAppState } from "./AppState";
import type { SurfaceId } from "./screens";

function SurfaceBody({ surface }: { surface: SurfaceId }) {
  switch (surface) {
    case "quick-actions":
      return <QuickActionsScreen />;
    case "subtitle-bar":
      return <SubtitleBarScreen />;
    case "compact-panel":
      return <CompactPanelScreen />;
  }
}

/** Renders one floating surface on its own, without the sidebar. */
export function SurfaceHost({ surface }: { surface: SurfaceId }) {
  const { state, dispatch } = useAppState();
  const t = useT();
  return (
    <div className="surface-host">
      <SurfaceBody surface={surface} />
      {/* An empty hash leaves the surface and shows the app. */}
      <a className="surface-host__back" href="#">
        {t("app.backToApp")}
      </a>
      {state.toast ? (
        <Toast
          message={state.toast.message}
          kind={state.toast.kind}
          onDismiss={() => dispatch({ type: "dismissToast" })}
        />
      ) : null}
    </div>
  );
}
