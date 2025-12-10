/**
 * Layout Component
 *
 * Main layout with header, tabs, and content area.
 */

import React from 'react';
import { Link, useLocation } from 'react-router-dom';
import type { GraphStats } from '../utils/graphProcessing';

interface LayoutProps {
  children: React.ReactNode;
  stats?: GraphStats;
  lastUpdated?: Date | null;
  branches?: string[];
  selectedBranch?: string | null;
  onBranchChange?: (branch: string | null) => void;
}

type ViewTab = 'chains' | 'timeline' | 'graph' | 'dag';

const TABS: { id: ViewTab; label: string; path: string }[] = [
  { id: 'chains', label: 'Chains', path: '/' },
  { id: 'timeline', label: 'Timeline', path: '/timeline' },
  { id: 'graph', label: 'Graph', path: '/graph' },
  { id: 'dag', label: 'DAG', path: '/dag' },
];

export const Layout: React.FC<LayoutProps> = ({ children, stats, lastUpdated, branches, selectedBranch, onBranchChange }) => {
  const location = useLocation();

  const getCurrentTab = (): ViewTab => {
    const path = location.pathname;
    if (path === '/timeline') return 'timeline';
    if (path === '/graph') return 'graph';
    if (path === '/dag') return 'dag';
    return 'chains';
  };

  const currentTab = getCurrentTab();

  return (
    <div style={styles.container}>
      {/* Header */}
      <header style={styles.header}>
        <div style={styles.headerContent}>
          <div>
            <h1 style={styles.title}>Deciduous</h1>
            <p style={styles.subtitle}>Decision Graph Viewer</p>
          </div>
          <nav style={styles.nav}>
            {TABS.map(tab => (
              <Link
                key={tab.id}
                to={tab.path}
                style={{
                  ...styles.tab,
                  ...(currentTab === tab.id ? styles.tabActive : {}),
                }}
              >
                {tab.label}
              </Link>
            ))}
          </nav>
          <div style={styles.navLinks}>
            <a href="https://github.com/notactuallytreyanastasio/losselot" target="_blank" rel="noopener noreferrer" style={styles.link}>
              GitHub
            </a>
          </div>
        </div>
      </header>

      {/* Stats Bar */}
      {stats && (
        <div style={styles.statsBar}>
          <div style={styles.stat}>
            <div style={styles.statNum}>{stats.nodeCount}</div>
            <div style={styles.statLabel}>Nodes</div>
          </div>
          <div style={styles.stat}>
            <div style={styles.statNum}>{stats.edgeCount}</div>
            <div style={styles.statLabel}>Edges</div>
          </div>
          <div style={styles.stat}>
            <div style={styles.statNum}>{stats.chainCount}</div>
            <div style={styles.statLabel}>Chains</div>
          </div>
          <div style={styles.stat}>
            <div style={styles.statNum}>{stats.sessionCount}</div>
            <div style={styles.statLabel}>Sessions</div>
          </div>
          {stats.linkedCommitCount > 0 && (
            <div style={styles.stat}>
              <div style={styles.statNum}>{stats.linkedCommitCount}</div>
              <div style={styles.statLabel}>Commits</div>
            </div>
          )}
          {/* Branch Filter */}
          {branches && branches.length > 0 && (
            <div style={{ ...styles.stat, marginLeft: 'auto' }}>
              <select
                value={selectedBranch || ''}
                onChange={(e) => onBranchChange?.(e.target.value || null)}
                style={styles.branchSelect}
              >
                <option value="">All branches</option>
                {branches.map(branch => (
                  <option key={branch} value={branch}>{branch}</option>
                ))}
              </select>
            </div>
          )}
          {lastUpdated && (
            <div style={{ ...styles.stat, marginLeft: branches?.length ? '10px' : 'auto' }}>
              <div style={{ ...styles.statLabel, fontSize: '10px' }}>
                Updated {lastUpdated.toLocaleTimeString()}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Main Content */}
      <main style={styles.main}>
        {children}
      </main>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    minHeight: '100vh',
    backgroundColor: '#1a1a2e',
    color: '#eee',
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
  },
  header: {
    backgroundColor: '#16213e',
    borderBottom: '1px solid #0f3460',
    padding: '0 20px',
  },
  headerContent: {
    display: 'flex',
    alignItems: 'center',
    gap: '30px',
    maxWidth: '1800px',
    margin: '0 auto',
    padding: '15px 0',
  },
  title: {
    fontSize: '20px',
    margin: 0,
    color: '#00d9ff',
  },
  subtitle: {
    fontSize: '12px',
    color: '#888',
    margin: '4px 0 0 0',
  },
  nav: {
    display: 'flex',
    gap: '4px',
    flex: 1,
  },
  tab: {
    padding: '10px 20px',
    fontSize: '13px',
    color: '#888',
    textDecoration: 'none',
    borderRadius: '6px 6px 0 0',
    backgroundColor: 'transparent',
    transition: 'all 0.2s',
  },
  tabActive: {
    backgroundColor: '#1a1a2e',
    color: '#00d9ff',
  },
  navLinks: {
    display: 'flex',
    gap: '15px',
  },
  link: {
    color: '#888',
    textDecoration: 'none',
    fontSize: '13px',
  },
  statsBar: {
    display: 'flex',
    gap: '20px',
    padding: '12px 20px',
    backgroundColor: '#16213e',
    margin: '15px',
    borderRadius: '8px',
    maxWidth: '1800px',
    marginLeft: 'auto',
    marginRight: 'auto',
  },
  stat: {
    textAlign: 'center',
    minWidth: '60px',
  },
  statNum: {
    fontSize: '20px',
    fontWeight: 'bold',
    color: '#00d9ff',
  },
  statLabel: {
    fontSize: '10px',
    color: '#888',
    textTransform: 'uppercase',
  },
  branchSelect: {
    backgroundColor: '#1a1a2e',
    color: '#00d9ff',
    border: '1px solid #0f3460',
    borderRadius: '4px',
    padding: '6px 10px',
    fontSize: '12px',
    cursor: 'pointer',
    minWidth: '120px',
  },
  main: {
    height: 'calc(100vh - 140px)',
    maxWidth: '1800px',
    margin: '0 auto',
    padding: '0 15px',
  },
};
