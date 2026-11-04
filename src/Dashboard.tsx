import { useState } from "react";
import type { ReactNode } from "react";
import { BookOpen, ChartColumn, History as HistoryIcon, SlidersHorizontal, Zap } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import Dictionary from "./Dictionary";
import History from "./History";
import Logo from "./Logo";
import type { PermissionsStatus } from "./PermissionDialog";
import type { ShortcutStatus } from "./ShortcutSetting";
import SettingsPage from "./SettingsPage";
import Stats from "./Stats";
import TopBar from "./TopBar";
import type { TopBarProps } from "./TopBar";
import "./dashboard.css";

type PageId = "history" | "dictionary" | "snippets" | "stats" | "settings";

const MAIN_PAGES: { id: PageId; label: string; icon: LucideIcon }[] = [
  { id: "history", label: "Historique", icon: HistoryIcon },
  { id: "dictionary", label: "Dictionnaire", icon: BookOpen },
  { id: "snippets", label: "Snippets", icon: Zap },
  { id: "stats", label: "Statistiques", icon: ChartColumn },
];

const SETTINGS_PAGE = { id: "settings" as const, label: "Réglages", icon: SlidersHorizontal };

type NavItemProps = {
  label: string;
  icon: LucideIcon;
  active: boolean;
  onClick: () => void;
};

function NavItem({ label, icon: Icon, active, onClick }: NavItemProps) {
  return (
    <button
      type="button"
      className="nav-item"
      data-active={active || undefined}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
      {label}
    </button>
  );
}

function Placeholder({ title }: { title: string }) {
  return (
    <>
      <h1 className="page-title">{title}</h1>
      <p className="page-soon">Cette page arrive dans un prochain lot.</p>
    </>
  );
}

type DashboardProps = TopBarProps & {
  onLearnShortcutChange: (status: ShortcutStatus) => void;
  permissions: PermissionsStatus | null;
};

function pageContent(page: PageId, props: DashboardProps): ReactNode {
  switch (page) {
    case "history":
      return <History />;
    case "settings":
      return (
        <SettingsPage
          shortcut={props.shortcut}
          onShortcutChange={props.onShortcutChange}
          learnShortcut={props.learnShortcut}
          onLearnShortcutChange={props.onLearnShortcutChange}
          permissions={props.permissions}
        />
      );
    case "dictionary":
      return <Dictionary />;
    case "snippets":
      return <Placeholder title="Snippets" />;
    case "stats":
      return <Stats />;
  }
}

/** The main window once the first launch is done: sidebar, top bar, and the current page. */
function Dashboard(props: DashboardProps) {
  const [page, setPage] = useState<PageId>("history");

  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Navigation">
        {/* Room for the macOS window buttons, which sit on top of the sidebar. */}
        <div className="sidebar-top" data-tauri-drag-region />
        <div className="sidebar-logo">
          <Logo height={26} />
        </div>
        <div className="nav-list">
          {MAIN_PAGES.map((item) => (
            <NavItem key={item.id} {...item} active={page === item.id} onClick={() => setPage(item.id)} />
          ))}
          <hr className="nav-separator" />
          <NavItem
            {...SETTINGS_PAGE}
            active={page === SETTINGS_PAGE.id}
            onClick={() => setPage(SETTINGS_PAGE.id)}
          />
        </div>
      </nav>
      <div className="main-column">
        <TopBar {...props} />
        <main className="page">{pageContent(page, props)}</main>
      </div>
    </div>
  );
}

export default Dashboard;
