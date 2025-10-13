import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './components/ui/card';
import { Button } from './components/ui/button';
import { Badge } from './components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs';
import { 
  Shield, 
  Server, 
  Users, 
  FileText, 
  ArrowUp, 
  Activity, 
  Settings,
  Code,
  GitBranch,
  Eye,
  Zap,
  Target,
  BarChart3,
  Lock,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Clock,
  Search,
  Menu,
  X
} from 'lucide-react';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Dashboard } from './components/Dashboard';
import { Tenants } from './components/Tenants';
import { Nodes } from './components/Nodes';
import { Plans } from './components/Plans';
import { Promotion } from './components/Promotion';
import { Telemetry } from './components/Telemetry';
import { Policies } from './components/Policies';
import { CodeIntelligence } from './components/CodeIntelligence';
import { Adapters } from './components/Adapters';
import { LoginForm } from './components/LoginForm';
// Contacts and Streams components - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §8
import { ContactsPage } from './components/ContactsPage';
import { TrainingStreamPage } from './components/TrainingStreamPage';
import { DiscoveryStreamPage } from './components/DiscoveryStreamPage';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './components/ui/select';
import { Toaster } from './components/ui/sonner';
import apiClient from './api/client';
import { User, UserRole, Tenant } from './api/types';

export default function App() {
  const [user, setUser] = useState<User | null>(null);
  const [activeTab, setActiveTab] = useState('dashboard');
  const [isDarkMode, setIsDarkMode] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedTenant, setSelectedTenant] = useState<string>('default');
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  useEffect(() => {
    // Load selected tenant from localStorage
    const savedTenant = localStorage.getItem('aos_selected_tenant');
    if (savedTenant) {
      setSelectedTenant(savedTenant);
    }

    // Check for existing session
    const checkAuth = async () => {
      const token = apiClient.getToken();
      if (token) {
        try {
          const currentUser = await apiClient.getCurrentUser();
          setUser({
            id: currentUser.user_id,
            email: currentUser.email,
            display_name: currentUser.email.split('@')[0], // Use email prefix as display name
            role: currentUser.role.charAt(0).toUpperCase() + currentUser.role.slice(1) as UserRole,
            tenant_id: 'default', // Default tenant for now
            permissions: [], // Empty permissions for now
          });

          // Load tenants list if user has permission
          if (['Admin', 'Operator', 'SRE'].includes(currentUser.role.charAt(0).toUpperCase() + currentUser.role.slice(1))) {
            try {
              const tenantsList = await apiClient.listTenants();
              setTenants(tenantsList);
            } catch (err) {
              console.error('Failed to load tenants:', err);
            }
          }
        } catch (err) {
          console.error('Failed to verify session:', err);
          apiClient.setToken(null);
        }
      }
      setIsLoading(false);
    };
    checkAuth();
  }, []);

  const handleLogin = async (credentials: { email: string; password: string }) => {
    setError(null);
    try {
      const response = await apiClient.login(credentials);
      // Create a user object from the login response
      setUser({
        id: response.user_id,
        email: credentials.email,
        display_name: credentials.email.split('@')[0], // Use email prefix as display name
        role: response.role.charAt(0).toUpperCase() + response.role.slice(1) as UserRole,
        tenant_id: 'default', // Default tenant for now
        permissions: [], // Empty permissions for now
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Login failed';
      setError(errorMessage);
      throw err;
    }
  };

  const handleLogout = async () => {
    try {
      await apiClient.logout();
    } catch (err) {
      console.error('Logout error:', err);
    }
    setUser(null);
    setActiveTab('dashboard');
  };

  const handleTenantChange = (tenantId: string) => {
    setSelectedTenant(tenantId);
    localStorage.setItem('aos_selected_tenant', tenantId);
  };

  const toggleTheme = () => {
    setIsDarkMode(!isDarkMode);
    document.documentElement.classList.toggle('dark');
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background flex-center">
        <div className="text-center">
          <Lock className="icon-large text-primary mx-auto mb-4 animate-pulse" />
          <p className="text-muted-foreground">Loading...</p>
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <div className={`min-h-screen bg-background ${isDarkMode ? 'dark' : ''}`}>
        <LoginForm onLogin={handleLogin} error={error} />
      </div>
    );
  }

  const navigationItems = [
    { id: 'dashboard', label: 'Dashboard', icon: BarChart3, roles: ['Admin', 'Operator', 'SRE', 'Compliance', 'Auditor', 'Viewer'] },
    { id: 'tenants', label: 'Tenants', icon: Users, roles: ['Admin'] },
    { id: 'nodes', label: 'Nodes', icon: Server, roles: ['Admin', 'Operator', 'SRE'] },
    { id: 'adapters', label: 'Adapters', icon: Code, roles: ['Admin', 'Operator', 'SRE', 'Viewer'] },
    { id: 'plans', label: 'Plans', icon: FileText, roles: ['Admin', 'Operator', 'SRE'] },
    { id: 'promotion', label: 'Promotion', icon: ArrowUp, roles: ['Admin', 'Operator'] },
    { id: 'telemetry', label: 'Telemetry', icon: Activity, roles: ['Admin', 'SRE', 'Compliance', 'Auditor'] },
    { id: 'policies', label: 'Policies', icon: Shield, roles: ['Admin', 'Compliance'] },
    { id: 'code', label: 'Code Intelligence', icon: GitBranch, roles: ['Admin', 'Operator', 'SRE', 'Viewer'] },
    // Contacts and Streams pages - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §8
    { id: 'contacts', label: 'Contacts', icon: Users, roles: ['Admin', 'Operator', 'SRE', 'Viewer'] },
    { id: 'training', label: 'Training', icon: Activity, roles: ['Admin', 'Operator', 'SRE'] },
    { id: 'discovery', label: 'Discovery', icon: Search, roles: ['Admin', 'Operator', 'SRE', 'Viewer'] }
  ].filter(item => item.roles.includes(user.role));

  return (
    <div className={`min-h-screen bg-background ${isDarkMode ? 'dark' : ''}`}>
      {/* Header */}
      <header className="border-b bg-card">
        <div className="flex h-16 items-center justify-between px-6">
          <div className="flex-center">
            <div className="flex-center">
              <Lock className="icon-standard text-primary" />
              <h1 className="font-medium">AdapterOS Control Plane</h1>
            </div>
            <div className="status-indicator status-success">
              Zero Egress Mode
            </div>
          </div>
          
          <div className="flex items-center gap-2">
            <Button 
              variant="ghost" 
              size="sm"
              className="md:hidden"
              onClick={() => setMobileMenuOpen(true)}
              aria-label="Open menu"
            >
              <Menu className="h-5 w-5" />
            </Button>
            
            {tenants.length > 0 && (
              <Select value={selectedTenant} onValueChange={handleTenantChange}>
                <SelectTrigger className="w-[180px] hidden sm:flex">
                  <SelectValue placeholder="Select tenant" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="default">Default</SelectItem>
                  {tenants.map((tenant) => (
                    <SelectItem key={tenant.id} value={tenant.id}>
                      {tenant.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
            <Badge variant="secondary" className="hidden sm:inline-flex">{user.role}</Badge>
            <span className="text-muted-foreground hidden md:inline">{user.email}</span>
            <Button variant="outline" size="sm" onClick={toggleTheme}>
              {isDarkMode ? '☀️' : '🌙'}
            </Button>
            <Button variant="outline" size="sm" onClick={handleLogout} className="hidden sm:inline-flex">
              Logout
            </Button>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex">
        {/* Sidebar Navigation */}
        <nav className={`
          w-64 border-r bg-card p-4
          md:block
          ${mobileMenuOpen ? 'fixed inset-y-0 left-0 z-50 block' : 'hidden'}
        `} aria-label="Main navigation">
          <div className="space-y-2">
            {/* Mobile close button */}
            <Button
              className="md:hidden mb-4 w-full justify-start"
              variant="ghost"
              onClick={() => setMobileMenuOpen(false)}
              aria-label="Close menu"
            >
              <X className="h-5 w-5 mr-2" />
              Close Menu
            </Button>
            
            {navigationItems.map((item) => {
              const Icon = item.icon;
              return (
                <Button
                  key={item.id}
                  variant={activeTab === item.id ? "default" : "ghost"}
                  className="w-full justify-start"
                  onClick={() => {
                    setActiveTab(item.id);
                    setMobileMenuOpen(false);
                  }}
                  aria-label={`Navigate to ${item.label}`}
                  aria-current={activeTab === item.id ? "page" : undefined}
                >
                  <Icon className="icon-standard mr-2" aria-hidden="true" />
                  {item.label}
                </Button>
              );
            })}
          </div>
        </nav>
        
        {/* Mobile overlay */}
        {mobileMenuOpen && (
          <div 
            className="fixed inset-0 bg-black/50 z-40 md:hidden"
            onClick={() => setMobileMenuOpen(false)}
            aria-hidden="true"
          />
        )}

        {/* Content Area */}
        <main className="flex-1 p-4 md:p-6">
          <ErrorBoundary>
            {activeTab === 'dashboard' && <Dashboard user={user} selectedTenant={selectedTenant} onNavigate={setActiveTab} />}
            {activeTab === 'tenants' && <Tenants user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'nodes' && <Nodes user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'adapters' && <Adapters user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'plans' && <Plans user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'promotion' && <Promotion user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'telemetry' && <Telemetry user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'policies' && <Policies user={user} selectedTenant={selectedTenant} />}
            {activeTab === 'code' && <CodeIntelligence user={user} selectedTenant={selectedTenant} />}
            {/* Contacts and Streams pages - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §8 */}
            {activeTab === 'contacts' && <ContactsPage selectedTenant={selectedTenant} />}
            {activeTab === 'training' && <TrainingStreamPage selectedTenant={selectedTenant} />}
            {activeTab === 'discovery' && <DiscoveryStreamPage selectedTenant={selectedTenant} />}
          </ErrorBoundary>
        </main>
      </div>
      <Toaster />
    </div>
  );
}