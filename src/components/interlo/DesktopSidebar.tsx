import {
  Bell,
  BookOpen,
  Building2,
  Check,
  CircleHelp,
  Compass,
  ChevronsUpDown,
  ChevronRight,
  ChevronUp,
  ExternalLink,
  Gift,
  Home,
  Languages,
  LogOut,
  MessageCircle,
  Mic2,
  Moon,
  Search,
  Settings,
  Smartphone,
  Sparkles,
  Sun,
  Users,
  UsersRound,
  X,
} from "lucide-react";
import { useRouter } from "next/router";
import { useEffect, useRef, useState, type ElementType, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "../../hooks/useTranslation";
import { getHelpUrl, getUsageLimitsUrl } from "../../lib/helpUrl";
import { updateUserPreferences } from "../../lib/userPreferencesApi";
import { LANGUAGES } from "../../pages/_components/LanguageSelector";
import { useAppDispatch, useAppSelector } from "../../redux/hooks";
import { useLogoutMutation } from "../../redux/queries/signupApi";
import { setCurrentLanguage, type Localization } from "../../redux/slices/languageSlice";
import { setRefetchUser, setUser } from "../../redux/slices/userSlice";
import { getLiveQuotaState } from "../../lib/minutesQuota";
import type { ProductTourTarget } from "./DesktopProductTour";

export type DesktopView =
  | "home"
  | "recordings"
  | "recording"
  | "recording-detail"
  | "people"
  | "person-detail"
  | "translated-file-detail"
  | "voice-typing"
  | "chat"
  | "translate"
  | "learning"
  | "team"
  | "pricing"
  | "referral";

type ComingSoonFeatureId = "companies";

const MIN_SIDEBAR_WIDTH = 145;
const DEFAULT_SIDEBAR_WIDTH = 165;
const PREVIOUS_DEFAULT_SIDEBAR_WIDTH = 180;
const MAX_SIDEBAR_WIDTH = 280;
const DISCORD_COMMUNITY_URL = "https://discord.gg/x9Yvu2sCed";
// Temporary internal QA entry; enable only while notification UI/UX is being tested.
const SHOW_NOTIFICATION_PREVIEW = false;

interface DesktopSidebarProps {
  activeView: DesktopView;
  collapsed: boolean;
  email?: string;
  firstName?: string;
  lastName?: string;
  tier?: string;
  onChangeView: (view: DesktopView) => void;
  onOpenExternal: (url: string) => void;
  onOpenSettings: (section?: "ai-notes") => void;
  onManageTeam: () => void;
  onOpenSearch: () => void;
  onOpenPricing: () => void;
  theme?: "dark" | "light";
  onToggleTheme?: () => void;
  productTourTarget?: ProductTourTarget;
  onStartProductTour: () => void;
}

const baseNavigation: Array<{
  id: "home" | "recordings" | "translate" | "learning" | "chat";
  icon: ElementType;
}> = [
  { id: "home", icon: Home },
  { id: "recordings", icon: Mic2 },
  { id: "translate", icon: Languages },
  { id: "learning", icon: BookOpen },
  { id: "chat", icon: MessageCircle },
];

const comingSoonFeatures: Array<{ id: ComingSoonFeatureId; icon: ElementType }> = [
  { id: "companies", icon: Building2 },
];

const PLATFORM_LANGUAGE_ORDER: Localization[] = [
  "en",
  "fr",
  "de",
  "in",
  "id",
  "it",
  "ja",
  "kr",
  "pt_br",
  "es_la",
  "es",
  "cn1",
  "zh_tw",
  "th",
  "vi",
  "pt",
  "ar",
  "uk",
  "pl",
  "tr",
  "ru",
];

const PLATFORM_LANGUAGE_LABELS: Partial<Record<Localization, string>> = {
  en: "English",
  fr: "Français",
  de: "Deutsch",
  in: "हिन्दी",
  id: "Bahasa Indonesia",
  it: "Italiano",
  ja: "日本語",
  kr: "한국어",
  pt: "Português (Portugal)",
  pt_br: "Português (Brasil)",
  es_la: "Español (Latinoamérica)",
  es: "Español (España)",
};

const PLATFORM_LANGUAGES = PLATFORM_LANGUAGE_ORDER.map((code) => ({
  code,
  label:
    PLATFORM_LANGUAGE_LABELS[code] ??
    LANGUAGES.find((language) => language.code === code)?.label ??
    code,
}));

const sidebarContentTransition = (collapsed: boolean) =>
  `min-w-0 overflow-hidden whitespace-nowrap transition-[max-width,opacity,transform] duration-200 ease-out motion-reduce:transition-none ${
    collapsed
      ? "max-w-0 -translate-x-1 opacity-0"
      : "max-w-[180px] translate-x-0 opacity-100 delay-100"
  }`;

function DiscordIcon({
  size = 17,
  className,
  color = "currentColor",
}: {
  size?: number;
  className?: string;
  color?: string;
}) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      className={className}
      fill={color}
    >
      <path d="M20.317 4.37a19.8 19.8 0 0 0-4.885-1.516.075.075 0 0 0-.079.038c-.21.375-.445.865-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.618-1.25.077.077 0 0 0-.078-.038A19.74 19.74 0 0 0 3.677 4.37a.07.07 0 0 0-.032.028C.533 9.046-.32 13.58.1 18.058a.083.083 0 0 0 .031.056c2.053 1.508 4.041 2.423 5.993 3.03a.078.078 0 0 0 .084-.028c.462-.63.873-1.295 1.226-1.994a.076.076 0 0 0-.042-.106 12.3 12.3 0 0 1-1.872-.892.077.077 0 0 1-.008-.128c.126-.094.252-.192.372-.291a.075.075 0 0 1 .078-.01c3.928 1.793 8.18 1.793 12.061 0a.075.075 0 0 1 .079.01c.12.099.246.198.373.292a.077.077 0 0 1-.007.128 12.3 12.3 0 0 1-1.873.891.077.077 0 0 0-.041.107c.36.698.772 1.363 1.225 1.993a.076.076 0 0 0 .084.029c1.961-.607 3.95-1.522 6.002-3.03a.077.077 0 0 0 .032-.055c.5-5.177-.839-9.674-3.549-13.66a.062.062 0 0 0-.031-.03ZM8.02 15.331c-1.183 0-2.157-1.086-2.157-2.419s.956-2.419 2.157-2.419c1.21 0 2.176 1.095 2.157 2.419 0 1.333-.956 2.419-2.157 2.419Zm7.975 0c-1.183 0-2.157-1.086-2.157-2.419s.956-2.419 2.157-2.419c1.21 0 2.176 1.095 2.157 2.419 0 1.333-.946 2.419-2.157 2.419Z" />
    </svg>
  );
}

const formatPlanTier = (tier: string | undefined, freeLabel: string): string => {
  const normalizedTier = tier?.trim().replace(/\s+plan$/i, "").toLowerCase();

  if (!normalizedTier || normalizedTier === "free" || normalizedTier === "freemium") {
    return freeLabel;
  }
  if (normalizedTier === "pro") return "Pro";
  if (normalizedTier === "premium") return "Premium";

  return tier!.trim().replace(/\s+plan$/i, "");
};

function UsageMeter({
  used,
  displayUsed,
  limit,
  label,
  theme,
  exhausted = false,
}: {
  used: number;
  displayUsed?: number;
  limit: number;
  label: string;
  theme: "dark" | "light";
  exhausted?: boolean;
}) {
  const { t } = useTranslation();
  const percentUsed = limit > 0 ? Math.min(100, Math.max(0, (used / limit) * 100)) : 0;

  return (
    <div>
      <div className={`h-1.5 w-full overflow-hidden rounded-full ${theme === "light" ? "bg-[#ECECEF]" : "bg-[#25252A]"}`}>
        <div
          className={`h-full rounded-full transition-[width] duration-300 ${
            exhausted
              ? "bg-gradient-to-r from-[#F59E0B] to-[#EF4444]"
              : "bg-gradient-to-r from-[#5156FF] to-[#7C3AED]"
          }`}
          style={{ width: `${percentUsed}%` }}
        />
      </div>
      <p className={`mt-1 text-[10px] leading-4 ${theme === "light" ? "text-[#4B5563]" : "text-[#A0A0A8]"}`}>
        {t("desktopUi.sidebar.usageOf", {
          used: (displayUsed ?? used).toLocaleString(),
          limit: limit.toLocaleString(),
          label,
        })}
      </p>
    </div>
  );
}

function SidebarTooltip({ label, children }: { label: string; children: ReactNode }) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);

  const showTooltip = () => {
    const bounds = anchorRef.current?.getBoundingClientRect();
    if (!bounds) return;
    const estimatedWidth = Math.max(60, label.length * 7.2 + 28);
    const halfWidth = estimatedWidth / 2;
    const centeredLeft = bounds.left + bounds.width / 2;
    setPosition({
      left: Math.min(
        window.innerWidth - halfWidth - 8,
        Math.max(halfWidth + 8, centeredLeft),
      ),
      top: bounds.top - 8,
    });
  };

  const tooltip = position && typeof document !== "undefined"
    ? createPortal(
        <span
          role="tooltip"
          className="pointer-events-none fixed z-[5000] -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-lg border border-[#3A3A40] bg-[#202024] px-2.5 py-1.5 text-[11px] font-semibold text-[#F1F1F3] shadow-[0_8px_24px_rgba(0,0,0,0.4)]"
          style={{ left: position.left, top: position.top }}
        >
          {label}
        </span>,
        document.body,
      )
    : null;

  return (
    <span
      ref={anchorRef}
      className="inline-flex"
      onMouseEnter={showTooltip}
      onMouseLeave={() => setPosition(null)}
      onFocusCapture={showTooltip}
      onBlurCapture={() => setPosition(null)}
    >
      {children}
      {tooltip}
    </span>
  );
}

export function DesktopSidebar({
  activeView,
  collapsed,
  email,
  firstName,
  lastName,
  tier,
  onChangeView,
  onOpenExternal,
  onOpenSettings,
  onManageTeam,
  onOpenSearch,
  onOpenPricing,
  theme = "dark",
  onToggleTheme,
  productTourTarget,
  onStartProductTour,
}: DesktopSidebarProps) {
  const { t, currentLanguage } = useTranslation();
  const dispatch = useAppDispatch();
  const user = useAppSelector((state) => state.user.user);
  const translationEnabled = useAppSelector((state) => state.recorder.enabled);
  const router = useRouter();
  const [logout] = useLogoutMutation();
  const initials = `${firstName?.[0] ?? "J"}${lastName?.[0] ?? ""}`.toUpperCase();
  const translationUsage = user?.translationUsage ?? 0;
  const translationLimit = user?.translationLimit ?? 0;
  const transcriptionUsage = user?.transcriptionUsage ?? 0;
  const transcriptionLimit = user?.transcriptionLimit ?? 0;
  const aiCreditsUsage = user?.AICreditsUsage ?? 0;
  const aiCreditsLimit = user?.AICreditsLimit ?? 0;
  const chatTokensUsage = user?.chatTokensUsage ?? 0;
  const chatTokensLimit = user?.chatTokensLimit ?? 0;
  const liveQuota = getLiveQuotaState({
    translationEnabled,
    translationUsage,
    translationLimit: user?.translationLimit ?? null,
    transcriptionUsage,
    transcriptionLimit: user?.transcriptionLimit ?? null,
  });
  const planTierLabel = formatPlanTier(tier, t("desktopSidebar.plan.free"));
  const navigation = baseNavigation;
  const [sidebarWidth, setSidebarWidth] = useState(DEFAULT_SIDEBAR_WIDTH);
  const [resizing, setResizing] = useState(false);
  const [comingSoonFeature, setComingSoonFeature] = useState<ComingSoonFeatureId | null>(null);
  const [mobileDownloadOpen, setMobileDownloadOpen] = useState(false);
  const [discordCommunityOpen, setDiscordCommunityOpen] = useState(false);
  const [profileMenuPosition, setProfileMenuPosition] = useState<{ left: number; bottom: number } | null>(null);
  const [languageMenuPosition, setLanguageMenuPosition] = useState<{
    left: number;
    top: number;
  } | null>(null);
  const [usagePopoverPosition, setUsagePopoverPosition] = useState<{
    left: number;
    bottom: number;
    width: number;
  } | null>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const profileButtonRef = useRef<HTMLButtonElement>(null);
  const languageButtonRef = useRef<HTMLButtonElement>(null);
  const collapsedUsageButtonRef = useRef<HTMLButtonElement>(null);
  const usageButtonRef = useRef<HTMLButtonElement>(null);
  const usageCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const languageCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const usageRefreshAtRef = useRef(0);
  const productTourProfileMenuOpenRef = useRef(false);

  const featureLabel = (id: ComingSoonFeatureId) => {
    return t(`desktopSidebar.features.${id}`);
  };

  const refreshUsage = () => {
    const now = Date.now();
    if (now - usageRefreshAtRef.current < 30_000) return;
    usageRefreshAtRef.current = now;
    dispatch(setRefetchUser());
  };

  const showUsagePopover = () => {
    if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
    const activeUsageButton = collapsed
      ? collapsedUsageButtonRef.current
      : usageButtonRef.current;
    const bounds = activeUsageButton?.getBoundingClientRect();
    if (!bounds) return;
    const sidebarBounds = sidebarRef.current?.getBoundingClientRect();
    const left = (sidebarBounds?.right ?? bounds.right) + 8;
    const availableWidth = Math.max(220, window.innerWidth - left - 12);
    setUsagePopoverPosition({
      left,
      bottom: Math.max(8, window.innerHeight - bounds.bottom - 20),
      width: Math.min(320, availableWidth),
    });
    refreshUsage();
  };

  const scheduleUsagePopoverClose = () => {
    if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
    usageCloseTimerRef.current = setTimeout(() => setUsagePopoverPosition(null), 140);
  };

  useEffect(() => {
    const storedWidth = Number(window.localStorage.getItem("MeetingCopilot-sidebar-width"));
    if (storedWidth >= MIN_SIDEBAR_WIDTH && storedWidth <= MAX_SIDEBAR_WIDTH) {
      setSidebarWidth(
        storedWidth === PREVIOUS_DEFAULT_SIDEBAR_WIDTH
          ? DEFAULT_SIDEBAR_WIDTH
          : storedWidth,
      );
    }
  }, []);

  useEffect(() => {
    if (!resizing) return;

    const handlePointerMove = (event: PointerEvent) => {
      const nextWidth = Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, event.clientX));
      setSidebarWidth(nextWidth);
    };
    const handlePointerUp = () => {
      setResizing(false);
      window.localStorage.setItem("MeetingCopilot-sidebar-width", String(sidebarWidth));
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
  }, [resizing, sidebarWidth]);

  useEffect(() => () => {
    if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
    if (languageCloseTimerRef.current) clearTimeout(languageCloseTimerRef.current);
  }, []);

  useEffect(() => {
    if (!comingSoonFeature && !mobileDownloadOpen && !discordCommunityOpen && !profileMenuPosition && !languageMenuPosition && !usagePopoverPosition) return;
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (languageMenuPosition) {
          setLanguageMenuPosition(null);
          return;
        }
        setUsagePopoverPosition(null);
        setComingSoonFeature(null);
        setMobileDownloadOpen(false);
        setDiscordCommunityOpen(false);
        setProfileMenuPosition(null);
      }
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [comingSoonFeature, discordCommunityOpen, languageMenuPosition, mobileDownloadOpen, profileMenuPosition, usagePopoverPosition]);

  useEffect(() => {
    if (!mobileDownloadOpen && !discordCommunityOpen) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [discordCommunityOpen, mobileDownloadOpen]);

  useEffect(() => {
    if (collapsed) setUsagePopoverPosition(null);
  }, [collapsed]);

  useEffect(() => {
    const shouldOpenProfileMenu = productTourTarget === "user-menu"
      || productTourTarget === "help-center"
      || productTourTarget === "product-tour";

    if (!shouldOpenProfileMenu || collapsed) {
      if (productTourProfileMenuOpenRef.current) {
        setProfileMenuPosition(null);
        setLanguageMenuPosition(null);
        productTourProfileMenuOpenRef.current = false;
      }
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      const bounds = profileButtonRef.current?.getBoundingClientRect();
      if (!bounds) return;
      const menuWidth = 230;
      setProfileMenuPosition({
        left: Math.max(8, Math.min(bounds.left, window.innerWidth - menuWidth - 8)),
        bottom: window.innerHeight - bounds.top + 8,
      });
      setLanguageMenuPosition(null);
      productTourProfileMenuOpenRef.current = true;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [collapsed, productTourTarget]);

  const toggleProfileMenu = () => {
    if (profileMenuPosition) {
      setProfileMenuPosition(null);
      setLanguageMenuPosition(null);
      return;
    }

    const bounds = profileButtonRef.current?.getBoundingClientRect();
    if (!bounds) return;
    const menuWidth = 230;
    setProfileMenuPosition({
      left: Math.max(8, Math.min(bounds.left, window.innerWidth - menuWidth - 8)),
      bottom: window.innerHeight - bounds.top + 8,
    });
  };

  const openProfileLink = (url: string) => {
    setProfileMenuPosition(null);
    setLanguageMenuPosition(null);
    onOpenExternal(url);
  };

  const showLanguageMenu = () => {
    if (languageCloseTimerRef.current) {
      clearTimeout(languageCloseTimerRef.current);
      languageCloseTimerRef.current = null;
    }
    const bounds = languageButtonRef.current?.getBoundingClientRect();
    if (!bounds) return;
    const menuWidth = 250;
    const menuHeight = Math.min(440, window.innerHeight - 16);
    const opensRight = bounds.right + 8 + menuWidth <= window.innerWidth;
    setLanguageMenuPosition({
      left: opensRight
        ? bounds.right + 8
        : Math.max(8, bounds.left - menuWidth - 8),
      top: Math.min(
        Math.max(8, bounds.top),
        Math.max(8, window.innerHeight - menuHeight - 8),
      ),
    });
  };

  const scheduleLanguageMenuClose = () => {
    if (languageCloseTimerRef.current) clearTimeout(languageCloseTimerRef.current);
    languageCloseTimerRef.current = setTimeout(() => {
      setLanguageMenuPosition(null);
      languageCloseTimerRef.current = null;
    }, 140);
  };

  const selectPlatformLanguage = async (code: Localization) => {
    setLanguageMenuPosition(null);
    if (code === currentLanguage) return;
    const previousLanguage = currentLanguage;
    const previousUser = user;
    dispatch(setCurrentLanguage(code));
    if (user) dispatch(setUser({ ...user, languagePreference: code }));
    try {
      await updateUserPreferences({ languagePreference: code });
    } catch {
      dispatch(setCurrentLanguage(previousLanguage));
      if (previousUser) dispatch(setUser(previousUser));
    }
  };

  const handleLogOut = async () => {
    setProfileMenuPosition(null);
    setLanguageMenuPosition(null);
    try {
      await logout({});
    } catch {
      // Clear the local session even if the server is unreachable.
    }
    localStorage.removeItem("accessToken");
    dispatch(setUser(null));
    window.location.assign("/home/");
  };

  return (
    <aside
      ref={sidebarRef}
      className={`desktop-sidebar relative flex h-full shrink-0 flex-col overflow-hidden bg-[#111113] py-3 text-[#F5F5F7] motion-reduce:transition-none ${
        collapsed ? "px-1.5" : "px-2.5"
      } ${
        resizing ? "transition-none" : "transition-[width] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
      }`}
      style={{ width: collapsed ? 48 : sidebarWidth }}
    >
      {/* When collapsed, the right divider starts below the window-chrome zone
          instead of running to the very top of the window; a horizontal line
          under the traffic lights meets it there, forming a connected corner. */}
      <div
        aria-hidden="true"
        className={`absolute bottom-0 right-0 w-px bg-[#2B2B30] transition-[top] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none ${collapsed ? "top-14" : "top-0"}`}
      />
      <div
        aria-hidden="true"
        className={`absolute left-0 right-3 top-11 h-px bg-[#2B2B30] transition-opacity duration-150 motion-reduce:transition-none ${collapsed ? "opacity-100 delay-150" : "opacity-0"}`}
      />
      <div
        aria-hidden="true"
        className={`absolute right-0 top-11 h-3 w-3 overflow-hidden bg-[#0B0B0D] transition-opacity duration-150 motion-reduce:transition-none ${collapsed ? "opacity-100 delay-150" : "opacity-0"}`}
      >
        <span className="absolute right-0 top-0 h-6 w-6 rounded-full border border-[#2B2B30] bg-[#111113]" />
      </div>
      {!collapsed && (
        <button
          type="button"
          aria-label={t("desktopSidebar.chrome.resizeNavigation")}
          onPointerDown={(event) => {
            event.preventDefault();
            setResizing(true);
          }}
          className={`absolute -right-1 top-0 z-20 h-full w-2 cursor-col-resize ${
            resizing ? "bg-[#5156FF]/40" : "bg-transparent hover:bg-[#5156FF]/25"
          }`}
        />
      )}

      {/* Draggable spacer under the native traffic lights / collapse chevron. */}
      <div style={{ WebkitAppRegion: "drag" } as React.CSSProperties} className="h-9 shrink-0" />
      <div className="mb-2.5 shrink-0" />

      <button
        type="button"
        onClick={onOpenSearch}
        className={`mb-2 flex h-9 items-center overflow-hidden rounded-lg border border-[#303036] bg-[#1A1A1D] text-xs text-[#A2A2AA] transition-[padding,gap,border-color,background-color] duration-300 hover:border-[#45454D] hover:bg-[#202024] motion-reduce:transition-none ${
          collapsed ? "justify-center px-0" : "gap-3 px-3"
        }`}
        aria-label={t("desktopSidebar.search.aria")}
      >
        <Search size={17} className="shrink-0" />
        <span aria-hidden={collapsed} className={sidebarContentTransition(collapsed)}>
          {t("desktopSidebar.search.label")}
        </span>
        <span
          aria-hidden={collapsed}
          className={`${collapsed ? "" : "ml-auto"} text-[11px] text-[#66666F] ${sidebarContentTransition(collapsed)}`}
        >
          ⌘ K
        </span>
      </button>

      <nav className="space-y-1">
        {navigation.map(({ id, icon: Icon }) => {
          const active =
            activeView === id
            || (activeView === "recording-detail" && id === "recordings")
            || (activeView === "translated-file-detail" && id === "translate");
          const label = t(`desktopSidebar.nav.${id}`);
          return (
            <button
              key={id}
              type="button"
              data-product-tour={id === "recordings" || id === "translate" ? id : undefined}
              onClick={() => onChangeView(id)}
              className={`flex h-9 w-full items-center overflow-hidden rounded-lg text-xs font-medium transition-[padding,gap,color,background-color] duration-300 motion-reduce:transition-none ${
                collapsed ? "justify-center px-0" : "gap-3 px-3"
              } ${
                active
                  ? "bg-gradient-to-r from-[#5156FF]/25 to-[#7C3AED]/10 text-[#9B9EFF]"
                  : "text-[#A0A0A8] hover:bg-[#202024] hover:text-white"
              }`}
              aria-label={label}
              aria-current={active ? "page" : undefined}
            >
              <Icon size={18} strokeWidth={active ? 2.4 : 2} className="shrink-0" />
              <span aria-hidden={collapsed} className={sidebarContentTransition(collapsed)}>{label}</span>
            </button>
          );
        })}
      </nav>

      {SHOW_NOTIFICATION_PREVIEW ? (
        <div className="mt-2 border-t border-[#2B2B30] pt-2">
          <button
            type="button"
            onClick={() => void router.push("/dashboard/notification-showcase")}
            className={`flex h-9 w-full items-center overflow-hidden rounded-lg text-xs font-medium text-[#8F92FF] transition-[padding,gap,color,background-color] duration-300 hover:bg-[#5156FF]/15 hover:text-[#B7B9FF] motion-reduce:transition-none ${
              collapsed ? "justify-center px-0" : "gap-3 px-3"
            }`}
            aria-label={t("desktopSidebar.notificationPreview")}
            title={collapsed ? t("desktopSidebar.notificationPreview") : undefined}
          >
            <Bell size={18} className="shrink-0" />
            <span
              aria-hidden={collapsed}
              className={sidebarContentTransition(collapsed)}
            >
              {t("desktopSidebar.notificationPreview")}
            </span>
          </button>
        </div>
      ) : null}

      <div className="mt-auto space-y-1 pt-3">
        <div className={`flex gap-1 ${collapsed ? "flex-col items-center" : "justify-between"}`}>
          <SidebarTooltip
            label={theme === "dark" ? t("desktopSidebar.theme.lightMode") : t("desktopSidebar.theme.darkMode")}
          >
            <button
              type="button"
              data-product-tour="theme"
              onClick={onToggleTheme}
              className="flex h-8 w-8 items-center justify-center rounded-lg text-[#8F8F98] hover:bg-[#202024] hover:text-white"
              aria-label={
                theme === "dark"
                  ? t("desktopSidebar.theme.switchToLight")
                  : t("desktopSidebar.theme.switchToDark")
              }
            >
              {theme === "dark" ? <Sun size={17} /> : <Moon size={17} />}
            </button>
          </SidebarTooltip>
          <div className={`flex gap-1 ${collapsed ? "flex-col items-center" : ""}`}>
            <SidebarTooltip label={t("desktopSidebar.features.people")}>
              <button
                type="button"
                data-product-tour="people"
                onClick={() => onChangeView("people")}
                className={`flex h-8 w-8 items-center justify-center rounded-lg transition ${
                  activeView === "people" || activeView === "person-detail"
                    ? "bg-[#292955] text-[#A5A7FF]"
                    : "text-[#8F8F98] hover:bg-[#202024] hover:text-white"
                }`}
                aria-label={t("desktopSidebar.features.people")}
                aria-current={activeView === "people" || activeView === "person-detail" ? "page" : undefined}
              >
                <Users size={17} />
              </button>
            </SidebarTooltip>
            {comingSoonFeatures.map(({ id, icon: Icon }) => {
              const label = t(`desktopSidebar.features.${id}`);
              return (
                <SidebarTooltip key={id} label={label}>
                  <button
                    type="button"
                    onClick={() => setComingSoonFeature(id)}
                    className="flex h-8 w-8 items-center justify-center rounded-lg text-[#8F8F98] hover:bg-[#202024] hover:text-white"
                    aria-label={label}
                  >
                    <Icon size={17} />
                  </button>
                </SidebarTooltip>
              );
            })}
          </div>
        </div>

        {/* Keep both plan variants mounted so minimize/maximize can cross-fade
            without either control popping into the layout. */}
        <div className="relative mt-2 h-9">
          <button
            ref={collapsedUsageButtonRef}
            type="button"
            onClick={() => {
              if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
              setUsagePopoverPosition(null);
              onOpenPricing();
            }}
            className={`absolute left-1/2 top-0 flex h-9 w-9 -translate-x-1/2 items-center justify-center rounded-xl transition-[opacity,transform] duration-200 motion-reduce:transition-none ${theme === "light" ? "bg-[#E9E9EC] text-[#1F2937] hover:bg-[#DEDEE2]" : "bg-[#26262C] text-white hover:bg-[#33333A]"} ${
              collapsed ? "scale-100 opacity-100 delay-100" : "pointer-events-none scale-90 opacity-0"
            }`}
            aria-label={t("desktopSidebar.plan.upgradePlan")}
            aria-haspopup="dialog"
            aria-expanded={collapsed && Boolean(usagePopoverPosition)}
            onMouseEnter={showUsagePopover}
            onMouseLeave={scheduleUsagePopoverClose}
            onFocus={showUsagePopover}
            onBlur={scheduleUsagePopoverClose}
            tabIndex={collapsed ? 0 : -1}
          >
            <Sparkles size={16} className="shrink-0" />
          </button>
          <div
            aria-hidden={collapsed}
            className={`absolute inset-0 flex items-center justify-between gap-1.5 rounded-xl border py-1 pl-3 pr-1 transition-[opacity,transform] duration-200 motion-reduce:transition-none ${theme === "light" ? "border-[#D5D5DB]" : "border-[#3A3A40]"} ${
              collapsed ? "pointer-events-none translate-x-1 opacity-0" : "translate-x-0 opacity-100 delay-100"
            }`}
          >
            <span className={`shrink-0 whitespace-nowrap text-[11px] font-semibold capitalize ${theme === "light" ? "text-[#1F2937]" : "text-[#D7D7DB]"}`}>
              {planTierLabel}
            </span>
            <span className="relative shrink-0">
              <button
                ref={usageButtonRef}
                type="button"
                data-minutes-upgrade="usage"
                className={`desktop-sidebar-usage-button rounded-lg border px-2 py-1 text-[10px] font-semibold focus-visible:outline focus-visible:outline-2 focus-visible:outline-[#7377FF] ${theme === "light" ? "border-[#292929] bg-[#292929] text-white hover:bg-[#3A3A3A]" : "border-[#45454D] bg-[#26262C] text-white hover:bg-[#33333A]"}`}
                tabIndex={collapsed ? -1 : 0}
                aria-haspopup="dialog"
                aria-expanded={Boolean(usagePopoverPosition)}
                onMouseEnter={showUsagePopover}
                onMouseLeave={scheduleUsagePopoverClose}
                onFocus={showUsagePopover}
                onBlur={scheduleUsagePopoverClose}
              >
                {t("desktopSidebar.usage.label")}
              </button>
            </span>
          </div>
        </div>

        <div className="mx-1 mt-1 border-t border-[#29292E]" />

        <button
          ref={profileButtonRef}
          type="button"
          onClick={toggleProfileMenu}
          className={`flex h-10 w-full items-center overflow-hidden rounded-lg transition-[padding,gap,background-color] duration-300 hover:bg-[#202024] motion-reduce:transition-none ${
            collapsed ? "justify-center px-0" : "gap-2.5 px-2"
          }`}
          aria-label={t("desktopSidebar.profile.openMenu")}
          aria-expanded={Boolean(profileMenuPosition)}
        >
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[#292955] text-xs font-bold text-[#A5A7FF]">
            {initials}
          </span>
          <span aria-hidden={collapsed} className={`flex flex-1 items-center gap-2.5 ${sidebarContentTransition(collapsed)}`}>
            <span className="min-w-0 flex-1 truncate text-left text-xs font-semibold text-[#F0F0F2]">
              {firstName || t("desktopSidebar.profile.userFallback")}
            </span>
            {profileMenuPosition
              ? <ChevronUp size={15} className="shrink-0 text-[#85858E]" />
              : <ChevronsUpDown size={15} className="shrink-0 text-[#85858E]" />}
          </span>
        </button>
      </div>

      {usagePopoverPosition && typeof document !== "undefined" && createPortal(
        <div
          role="dialog"
          aria-label={t("desktopSidebar.usage.planUsage")}
          onMouseEnter={() => {
            if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
          }}
          onMouseLeave={scheduleUsagePopoverClose}
          onFocusCapture={() => {
            if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
          }}
          onBlurCapture={scheduleUsagePopoverClose}
          className={`fixed z-[5500] rounded-2xl border px-5 pt-5 pb-2 text-left ${theme === "light" ? "border-[#DEDEE2] bg-[#FFFFFF] shadow-[0_18px_45px_rgba(31,41,55,0.18)]" : "border-[#3A3A40] bg-[#17171A] shadow-[0_18px_50px_rgba(0,0,0,0.6)]"}`}
          style={{
            left: usagePopoverPosition.left,
            bottom: usagePopoverPosition.bottom,
            width: usagePopoverPosition.width,
          }}
        >
          <div className="space-y-4">
            <UsageMeter
              used={translationUsage}
              limit={translationLimit}
              label={t("desktopSidebar.usage.translationMinutes")}
              theme={theme}
              exhausted={liveQuota.translationExhausted}
            />
            <UsageMeter
              used={transcriptionUsage}
              displayUsed={Math.ceil(transcriptionUsage)}
              limit={transcriptionLimit}
              label={t("desktopSidebar.usage.transcriptionMinutes")}
              theme={theme}
              exhausted={liveQuota.transcriptionExhausted}
            />
            <UsageMeter used={aiCreditsUsage} limit={aiCreditsLimit} label={t("desktopSidebar.usage.aiCredits")} theme={theme} />
            <UsageMeter used={chatTokensUsage} limit={chatTokensLimit} label={t("desktopSidebar.usage.chatTokens")} theme={theme} />
          </div>
          <button
            type="button"
            onClick={() => {
              if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
              setUsagePopoverPosition(null);
              onOpenPricing();
            }}
            className="mt-5 w-full rounded-xl bg-gradient-to-r from-[#5156FF] to-[#7C3AED] px-3 py-3 text-sm font-semibold text-white shadow-[0_8px_22px_rgba(81,86,255,0.28)] transition-[transform,box-shadow,filter] duration-150 hover:-translate-y-0.5 hover:brightness-110 hover:shadow-[0_12px_28px_rgba(81,86,255,0.38)] active:translate-y-0 active:brightness-95 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#7377FF]"
          >
            {t("desktopSidebar.plan.upgrade")}
          </button>
          <a
            href={getUsageLimitsUrl(currentLanguage)}
            onClick={(event) => {
              event.preventDefault();
              if (usageCloseTimerRef.current) clearTimeout(usageCloseTimerRef.current);
              setUsagePopoverPosition(null);
              onOpenExternal(getUsageLimitsUrl(currentLanguage));
            }}
            className={`mt-1.5 flex min-h-6 items-center justify-center gap-1.5 rounded text-[11px] leading-5 transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#7377FF] ${theme === "light" ? "text-[#777782] hover:text-[#4D4D57]" : "text-[#8D8D98] hover:text-[#B9B9C3]"}`}
          >
            <span>{t("desktopSidebar.usage.guide")}</span>
            <ExternalLink size={11} className="shrink-0 opacity-70" aria-hidden="true" />
          </a>
        </div>,
        document.body,
      )}

      {profileMenuPosition && typeof document !== "undefined" && createPortal(
        <>
          <button
            type="button"
            className="fixed inset-0 z-[5400] cursor-default bg-transparent"
            onClick={() => {
              setProfileMenuPosition(null);
              setLanguageMenuPosition(null);
            }}
            aria-label={t("desktopSidebar.profile.closeMenu")}
          />
          <div
            role="menu"
            data-product-tour="user-menu"
            className={`fixed z-[5500] w-[230px] overflow-hidden rounded-xl border p-1.5 shadow-[0_20px_55px_rgba(0,0,0,0.25)] ${
              theme === "light"
                ? "border-[#D5D5DB] bg-[#FFFFFF] text-[#111827]"
                : "border-[#3A3A40] bg-[#1A1A1D] text-[#EDEDF0]"
            }`}
            style={{ left: profileMenuPosition.left, bottom: profileMenuPosition.bottom }}
          >
            <p className="truncate px-3 pb-2 pt-1 text-xs font-medium text-[#85858E]">
              {email || t("desktopSidebar.profile.notSignedIn")}
            </p>
            {[
              {
                key: "settings",
                label: t("desktopSidebar.profile.settings"),
                icon: Settings,
                showsChevron: false,
                showsExternal: false,
                action: () => {
                  setProfileMenuPosition(null);
                  setLanguageMenuPosition(null);
                  onOpenSettings();
                },
              },
              ...(user?.isTeamAdmin ? [{
                key: "team",
                label: t("desktopUi.sidebar.manageTeam"),
                icon: UsersRound,
                showsChevron: false,
                showsExternal: false,
                action: () => {
                  setProfileMenuPosition(null);
                  setLanguageMenuPosition(null);
                  onManageTeam();
                },
              }] : []),
              {
                key: "languages",
                label: t("desktopSidebar.profile.languages"),
                icon: Languages,
                showsChevron: true,
                showsExternal: false,
                action: showLanguageMenu,
              },
              {
                key: "help",
                label: t("desktopSidebar.profile.helpCenter"),
                icon: CircleHelp,
                showsChevron: false,
                showsExternal: true,
                action: () => openProfileLink(getHelpUrl(currentLanguage)),
              },
              {
                key: "productTour",
                label: t("desktopSidebar.profile.productTour"),
                icon: Compass,
                showsChevron: false,
                showsExternal: false,
                action: () => {
                  setProfileMenuPosition(null);
                  setLanguageMenuPosition(null);
                  onStartProductTour();
                },
              },
            ].map(({ key, label, icon: Icon, showsChevron, showsExternal, action }) => (
              <button
                key={key}
                data-product-tour={
                  key === "help"
                      ? "help-center"
                      : key === "productTour"
                        ? "product-tour"
                        : undefined
                }
                ref={key === "languages" ? languageButtonRef : undefined}
                type="button"
                role="menuitem"
                onClick={action}
                aria-expanded={key === "languages" ? Boolean(languageMenuPosition) : undefined}
                aria-haspopup={key === "languages" ? "menu" : undefined}
                onMouseEnter={key === "languages" ? showLanguageMenu : undefined}
                onMouseLeave={key === "languages" ? scheduleLanguageMenuClose : undefined}
                onFocus={key === "languages" ? showLanguageMenu : undefined}
                onBlur={key === "languages" ? scheduleLanguageMenuClose : undefined}
                className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-xs font-medium ${
                  theme === "light" ? "hover:bg-[#EFEFF1]" : "hover:bg-[#29292E]"
                } ${
                  key === "languages" && languageMenuPosition
                    ? theme === "light"
                      ? "bg-[#E7E8FF] text-[#111827]"
                      : "bg-[#29292E]"
                    : ""
                }`}
              >
                <Icon size={17} className="shrink-0 text-[#A0A0A8]" />
                <span className="flex-1">{label}</span>
                {showsChevron && <ChevronRight size={15} className="shrink-0 text-[#777780]" />}
                {showsExternal && (
                  <ExternalLink aria-hidden="true" size={15} className="shrink-0 text-[#777780]" />
                )}
              </button>
            ))}

            <div className="my-2 border-t border-[#303036]" />

            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setProfileMenuPosition(null);
                setLanguageMenuPosition(null);
                setMobileDownloadOpen(true);
              }}
              className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-xs font-medium ${theme === "light" ? "hover:bg-[#EFEFF1]" : "hover:bg-[#29292E]"}`}
            >
              <Smartphone size={17} className="shrink-0 text-[#A0A0A8]" />
              <span>{t("desktopSidebar.profile.downloadMobileApps")}</span>
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setProfileMenuPosition(null);
                setLanguageMenuPosition(null);
                onChangeView("referral");
              }}
              className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-xs font-medium ${theme === "light" ? "hover:bg-[#EFEFF1]" : "hover:bg-[#29292E]"}`}
            >
              <Gift size={17} className="shrink-0 text-[#A0A0A8]" />
              <span>{t("desktopSidebar.profile.giftMeetingCopilot")}</span>
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setProfileMenuPosition(null);
                setLanguageMenuPosition(null);
                setDiscordCommunityOpen(true);
              }}
              className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-xs font-medium ${theme === "light" ? "hover:bg-[#EFEFF1]" : "hover:bg-[#29292E]"}`}
            >
              <DiscordIcon size={17} className="shrink-0 text-[#A0A0A8]" />
              <span className="flex-1">{t("desktopSidebar.profile.joinDiscordCommunity")}</span>
            </button>

            <div className="my-2 border-t border-[#303036]" />

            <button
              type="button"
              role="menuitem"
              onClick={handleLogOut}
              className={`flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-xs font-medium ${theme === "light" ? "hover:bg-[#EFEFF1]" : "hover:bg-[#29292E]"}`}
            >
              <LogOut size={17} className="shrink-0 text-[#A0A0A8]" />
              <span>{t("desktopSidebar.profile.logOut")}</span>
            </button>
          </div>
          {languageMenuPosition ? (
            <div
              role="menu"
              aria-label={t("desktopSidebar.profile.languages")}
              onMouseEnter={() => {
                if (languageCloseTimerRef.current) {
                  clearTimeout(languageCloseTimerRef.current);
                  languageCloseTimerRef.current = null;
                }
              }}
              onMouseLeave={scheduleLanguageMenuClose}
              onFocusCapture={() => {
                if (languageCloseTimerRef.current) {
                  clearTimeout(languageCloseTimerRef.current);
                  languageCloseTimerRef.current = null;
                }
              }}
              onBlurCapture={scheduleLanguageMenuClose}
              className={`fixed z-[5600] max-h-[min(440px,calc(100vh-16px))] w-[250px] overflow-y-auto rounded-xl border p-1.5 shadow-[0_20px_55px_rgba(0,0,0,0.25)] ${
                theme === "light"
                  ? "border-[#D5D5DB] bg-[#FFFFFF] text-[#111827]"
                  : "border-[#3A3A40] bg-[#1A1A1D] text-[#EDEDF0]"
              }`}
              style={{
                left: languageMenuPosition.left,
                top: languageMenuPosition.top,
              }}
            >
              {PLATFORM_LANGUAGES.map((language) => {
                const selected = language.code === currentLanguage;
                return (
                  <button
                    key={language.code}
                    type="button"
                    role="menuitemradio"
                    aria-checked={selected}
                    onClick={() => void selectPlatformLanguage(language.code)}
                    className={`flex min-h-9 w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition ${
                      theme === "light" ? "hover:bg-[#EFEFF1]" : "hover:bg-[#29292E]"
                    } ${
                      selected
                        ? theme === "light"
                          ? "bg-[#E7E8FF] text-[#111827]"
                          : "bg-[#242429] text-white"
                        : theme === "light"
                          ? "text-[#111827]"
                          : "text-[#D7D7DB]"
                    }`}
                  >
                    <span className="min-w-0 flex-1">{language.label}</span>
                    {selected ? (
                      <Check size={16} className="shrink-0 text-[#7C80FF]" />
                    ) : null}
                  </button>
                );
              })}
            </div>
          ) : null}
        </>,
        document.body,
      )}

      {mobileDownloadOpen && typeof document !== "undefined" && createPortal(
        <div
          className="font-quicksand fixed inset-0 z-[6000] flex items-center justify-center bg-black/60 p-5 backdrop-blur-[3px]"
          onMouseDown={() => setMobileDownloadOpen(false)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="mobile-download-title"
            aria-describedby="mobile-download-description"
            className={`relative w-full max-w-[400px] rounded-[24px] border px-6 pb-7 pt-7 text-center shadow-[0_28px_80px_rgba(0,0,0,0.38)] ${
              theme === "light"
                ? "border-[#DCDCE1] bg-[#FFFFFF]"
                : "border-[#38383F] bg-[#17171A]"
            }`}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <button
              type="button"
              autoFocus
              onClick={() => setMobileDownloadOpen(false)}
              className={`absolute right-4 top-4 flex h-9 w-9 items-center justify-center rounded-full transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#5156FF] ${
                theme === "light"
                  ? "text-[#717179] hover:bg-[#ECECEF] hover:text-[#222321]"
                  : "text-[#85858E] hover:bg-[#25252A] hover:text-white"
              }`}
              aria-label={t("desktopSidebar.mobile.close")}
            >
              <X size={21} strokeWidth={2.2} />
            </button>

            <span className={`mx-auto flex h-10 w-10 items-center justify-center rounded-xl ${
              theme === "light"
                ? "bg-[#ECECFF] text-[#5156FF]"
                : "bg-[#292955] text-[#A5A7FF]"
            }`}>
              <Smartphone aria-hidden="true" size={20} strokeWidth={2.2} />
            </span>
            <h2
              id="mobile-download-title"
              className={`mt-3 px-7 text-[22px] font-bold leading-[1.25] tracking-[-0.02em] ${
                theme === "light" ? "text-[#292A28]" : "text-[#F1F1F3]"
              }`}
            >
              {t("desktopSidebar.mobile.title")}
            </h2>
            <p
              id="mobile-download-description"
              className={`mt-1.5 text-[13px] font-medium leading-5 ${
                theme === "light" ? "text-[#77777E]" : "text-[#A0A0A8]"
              }`}
            >
              {t("desktopSidebar.mobile.subtitle")}
            </p>

            <div className={`mx-auto mt-5 w-full max-w-[270px] rounded-[20px] border bg-white p-4 ${
              theme === "light"
                ? "border-[#E2E2E6] shadow-[0_8px_24px_rgba(24,24,28,0.06)]"
                : "border-[#44444C] shadow-[0_12px_32px_rgba(0,0,0,0.35)]"
            }`}>
              {/* A static, high-resolution QR keeps scanning reliable without a third-party request. */}
              <img
                src="/images/MeetingCopilot-mobile-download-qr.png"
                alt={t("desktopSidebar.mobile.qrAlt")}
                className="aspect-square h-auto w-full"
              />
            </div>
          </div>
        </div>,
        document.body,
      )}

      {discordCommunityOpen && typeof document !== "undefined" && createPortal(
        <div
          className="font-quicksand fixed inset-0 z-[6000] flex items-center justify-center bg-black/60 p-5 backdrop-blur-[3px]"
          onMouseDown={() => setDiscordCommunityOpen(false)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="discord-community-title"
            aria-describedby="discord-community-description"
            className={`relative w-full max-w-[400px] rounded-[24px] border px-6 pb-7 pt-7 text-center shadow-[0_28px_80px_rgba(0,0,0,0.38)] ${
              theme === "light"
                ? "border-[#DCDCE1] bg-[#FFFFFF]"
                : "border-[#38383F] bg-[#17171A]"
            }`}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <button
              type="button"
              onClick={() => setDiscordCommunityOpen(false)}
              className={`absolute right-4 top-4 flex h-9 w-9 items-center justify-center rounded-full transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#5865F2] ${
                theme === "light"
                  ? "text-[#717179] hover:bg-[#ECECEF] hover:text-[#222321]"
                  : "text-[#85858E] hover:bg-[#25252A] hover:text-white"
              }`}
              aria-label={t("desktopSidebar.comingSoon.close")}
            >
              <X size={21} strokeWidth={2.2} />
            </button>

            <span className="mx-auto flex h-12 w-12 items-center justify-center rounded-2xl bg-[#5865F2] text-white shadow-[0_10px_30px_rgba(88,101,242,0.3)]">
              <DiscordIcon size={25} color="#FFFFFF" />
            </span>
            <h2
              id="discord-community-title"
              className={`mt-4 px-7 text-[22px] font-bold leading-[1.25] tracking-[-0.02em] ${
                theme === "light" ? "text-[#292A28]" : "text-[#F1F1F3]"
              }`}
            >
              {t("desktopSidebar.profile.joinDiscordCommunity")}
            </h2>
            <p
              id="discord-community-description"
              className={`mx-auto mt-2 max-w-[310px] text-[13px] font-medium leading-5 ${
                theme === "light" ? "text-[#77777E]" : "text-[#A0A0A8]"
              }`}
            >
              {t("desktopSidebar.discord.description")}
            </p>
            <button
              type="button"
              autoFocus
              onClick={() => {
                setDiscordCommunityOpen(false);
                onOpenExternal(DISCORD_COMMUNITY_URL);
              }}
              style={{ color: "#FFFFFF" }}
              className="mt-6 inline-flex h-11 w-full items-center justify-center gap-2 rounded-xl bg-[#5865F2] px-5 text-sm font-semibold text-white shadow-[0_10px_26px_rgba(88,101,242,0.25)] transition hover:bg-[#4752C4] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#5865F2]"
            >
              <DiscordIcon size={18} color="#FFFFFF" />
              <span style={{ color: "#FFFFFF" }}>{t("desktopSidebar.profile.joinDiscordCommunity")}</span>
              <ExternalLink aria-hidden="true" size={15} color="#FFFFFF" />
            </button>
          </div>
        </div>,
        document.body,
      )}

      {comingSoonFeature && typeof document !== "undefined" && createPortal(
        <div
          className="fixed inset-0 z-[6000] flex items-center justify-center bg-black/65 p-5 backdrop-blur-[3px]"
          onMouseDown={() => setComingSoonFeature(null)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="coming-soon-title"
            className="relative w-full max-w-[360px] rounded-2xl border border-[#38383F] bg-[#17171A] p-6 text-center shadow-[0_24px_70px_rgba(0,0,0,0.6)]"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <button
              type="button"
              onClick={() => setComingSoonFeature(null)}
              className="absolute right-3 top-3 flex h-8 w-8 items-center justify-center rounded-lg text-[#85858E] hover:bg-[#25252A] hover:text-white"
              aria-label={t("desktopSidebar.comingSoon.close")}
            >
              <X size={16} />
            </button>
            <span className="mx-auto flex h-12 w-12 items-center justify-center rounded-2xl bg-[#292955] text-[#A5A7FF]">
              <Building2 size={22} />
            </span>
            <h2 id="coming-soon-title" className="mt-4 text-lg font-bold text-[#F1F1F3]">
              {t("desktopSidebar.comingSoon.title", { feature: featureLabel(comingSoonFeature) })}
            </h2>
            <p className="mt-2 text-sm leading-6 text-[#92929B]">
              {t("desktopSidebar.comingSoon.workspaceBody", {
                feature: featureLabel(comingSoonFeature),
              })}
            </p>
            <button
              type="button"
              onClick={() => setComingSoonFeature(null)}
              className="mt-6 h-10 rounded-xl bg-[#5156FF] px-6 text-sm font-semibold text-white hover:bg-[#6266FF]"
            >
              {t("desktopSidebar.comingSoon.gotIt")}
            </button>
          </div>
        </div>,
        document.body,
      )}
    </aside>
  );
}
