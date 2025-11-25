import React, { useState, useEffect, useCallback } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Progress } from './ui/progress';
import { Lock, Shield, AlertTriangle, XCircle, Zap, Clock, Server, CheckCircle2, XCircle as XCircleIcon, Loader2, RefreshCw } from 'lucide-react';
import { Alert, AlertDescription } from './ui/alert';
import { apiClient } from '../api/client';
import { LoginFormSchema, type LoginFormData } from '../schemas/common.schema';
import { useServiceStatus } from '../hooks/useServiceStatus';
import { cn } from './ui/utils';
import type { HealthResponse } from '../api/api-types';

interface LoginFormProps {
  onLogin: (credentials: { email: string; password: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
}

interface ServiceStatus {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'starting' | 'error';
}

type BackendState = 'checking' | 'loading' | 'ready' | 'error';

export function LoginForm({ onLogin, onDevBypass, error }: LoginFormProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [isDevBypassLoading, setIsDevBypassLoading] = useState(false);
  const [currentTime, setCurrentTime] = useState(new Date());
  const [backendState, setBackendState] = useState<BackendState>('checking');
  const [backendHealth, setBackendHealth] = useState<HealthResponse | null>(null);
  const [serviceStatuses, setServiceStatuses] = useState<ServiceStatus[]>([]);
  const [checkAttempts, setCheckAttempts] = useState(0);
  const [loadingProgress, setLoadingProgress] = useState(0);
  const isDev = import.meta.env.DEV;

  const { status: serviceStatus } = useServiceStatus();

  // Update time ticker every second
  useEffect(() => {
    const timer = setInterval(() => {
      setCurrentTime(new Date());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  // Check backend health and determine state
  const checkBackendHealth = useCallback(async () => {
    try {
      const health = await apiClient.get<HealthResponse>('/healthz');
      setBackendHealth(health);
      
      if (health.status === 'healthy') {
        setBackendState('ready');
        setLoadingProgress(100);
      } else if (health.status === 'degraded') {
        // Degraded but functional - allow login
        setBackendState('ready');
        setLoadingProgress(90);
      } else {
        setBackendState('loading');
        setLoadingProgress(50);
      }
    } catch (err) {
      const attempts = checkAttempts + 1;
      setCheckAttempts(attempts);
      
      // If we've tried multiple times, show error
      if (attempts >= 3) {
        setBackendState('error');
        setLoadingProgress(0);
      } else {
        // Still loading - backend might be starting
        setBackendState('loading');
        setLoadingProgress(Math.min(attempts * 25, 75));
      }
    }
  }, [checkAttempts]);

  // Initial backend health check
  useEffect(() => {
    checkBackendHealth();
  }, []);

  // Poll backend health while loading
  useEffect(() => {
    if (backendState === 'loading' || backendState === 'checking') {
      const interval = setInterval(() => {
        checkBackendHealth();
      }, 2000); // Check every 2 seconds
      return () => clearInterval(interval);
    }
  }, [backendState, checkBackendHealth]);

  // Update service statuses from API (only when backend is ready)
  useEffect(() => {
    if (backendState !== 'ready') {
      // Set minimal service info while loading
      setServiceStatuses([
        { id: 'backend', name: 'Backend Server', status: backendState === 'error' ? 'error' : backendState === 'loading' ? 'starting' : 'stopped' },
        { id: 'ui', name: 'UI Frontend', status: 'running' },
      ]);
      return;
    }

    // Backend is ready - get full service status
    if (serviceStatus?.services && serviceStatus.services.length > 0) {
      const services: ServiceStatus[] = serviceStatus.services.map((s: any) => ({
        id: s.id || s.name?.toLowerCase().replace(/\s+/g, '-') || 'unknown',
        name: s.name || s.id || 'Unknown Service',
        status: s.state === 'running' ? 'running' : s.state === 'failed' ? 'error' : s.state === 'starting' ? 'starting' : 'stopped',
      }));
      setServiceStatuses(services);
    } else if (backendHealth) {
      // Use health check data to infer service status
      const services: ServiceStatus[] = [
        { id: 'backend', name: 'Backend Server', status: backendHealth.status === 'healthy' ? 'running' : backendHealth.status === 'degraded' ? 'running' : 'error' },
        { id: 'ui', name: 'UI Frontend', status: 'running' },
      ];
      
      // Add component health as services
      if (backendHealth.components) {
        Object.entries(backendHealth.components).forEach(([name, component]) => {
          services.push({
            id: name.toLowerCase().replace(/\s+/g, '-'),
            name: name,
            status: component.status === 'healthy' ? 'running' : component.status === 'degraded' ? 'running' : 'error',
          });
        });
      }
      
      setServiceStatuses(services);
    }
  }, [backendState, serviceStatus, backendHealth]);

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
    watch,
  } = useForm<LoginFormData>({
    resolver: zodResolver(LoginFormSchema),
    mode: 'onChange',
    defaultValues: {
      email: '',
      password: '',
    },
  });

  const watchedFields = watch();

  const onSubmit = async (data: LoginFormData) => {
    setIsLoading(true);
    try {
      await onLogin({
        email: data.email.trim(),
        password: data.password.trim(),
      });
    } catch (err) {
      // Error is handled by parent component
    } finally {
      setIsLoading(false);
    }
  };

  const handleDevBypass = async () => {
    setIsDevBypassLoading(true);
    try {
      const response = await apiClient.devBypass();
      if (!response.token || !response.user_id || !response.tenant_id || !response.role) {
        throw new Error('Dev bypass response missing required authentication fields');
      }
      if (onDevBypass) {
        await onDevBypass();
      }
    } catch (err) {
      console.error('Dev bypass failed:', err instanceof Error ? err.message : String(err));
    } finally {
      setIsDevBypassLoading(false);
    }
  };

  const handleRetry = () => {
    setCheckAttempts(0);
    setBackendState('checking');
    setLoadingProgress(0);
    checkBackendHealth();
  };

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString('en-US', { 
      hour12: false, 
      hour: '2-digit', 
      minute: '2-digit', 
      second: '2-digit' 
    });
  };

  const getServiceStatusIcon = (status: ServiceStatus['status']) => {
    switch (status) {
      case 'running':
        return <CheckCircle2 className="h-4 w-4 text-green-500" />;
      case 'starting':
        return <Loader2 className="h-4 w-4 text-blue-500 animate-spin" />;
      case 'error':
        return <XCircleIcon className="h-4 w-4 text-red-500" />;
      default:
        return <XCircleIcon className="h-4 w-4 text-gray-400" />;
    }
  };

  const runningServices = serviceStatuses.filter(s => s.status === 'running').length;
  const totalServices = serviceStatuses.length || 1;
  const serviceProgress = totalServices > 0 ? (runningServices / totalServices) * 100 : 0;

  // Show loading screen until backend is ready
  if (backendState !== 'ready') {
    return (
      <div className="min-h-screen flex items-center justify-center p-6 bg-background">
        <div className="w-full max-w-md space-y-6">
          {/* Header */}
          <div className="text-center space-y-4">
            <div className="flex justify-center">
              <div className="flex items-center justify-center bg-primary text-primary-foreground p-3 rounded-lg">
                <Lock className="h-6 w-6" />
                <span className="font-medium ml-2">AdapterOS</span>
              </div>
            </div>
            <div className="space-y-2">
              <h1 className="font-medium text-xl">Control Plane Access</h1>
              <p className="text-muted-foreground text-sm">
                Secure, air-gapped system management
              </p>
            </div>
          </div>

          {/* Time Ticker */}
          <div className="flex items-center justify-center space-x-2 text-muted-foreground">
            <Clock className="h-4 w-4" />
            <span className="font-mono text-sm">{formatTime(currentTime)}</span>
          </div>

          {/* Loading Card */}
          <Card className="bg-muted/30">
            <CardHeader className="pb-3">
              <CardTitle className="text-base flex items-center gap-2">
                <Server className="h-4 w-4" />
                {backendState === 'error' ? 'Service Unavailable' : 'Initializing Services'}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {backendState === 'error' ? (
                <>
                  <Alert variant="destructive">
                    <XCircle className="h-4 w-4" />
                    <AlertDescription>
                      Backend server is not responding. Please ensure the server is running.
                    </AlertDescription>
                  </Alert>
                  <Button onClick={handleRetry} className="w-full" variant="outline">
                    <RefreshCw className="h-4 w-4 mr-2" />
                    Retry Connection
                  </Button>
                </>
              ) : (
                <>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">Checking backend health...</span>
                      <span className="font-mono text-xs text-muted-foreground">
                        Attempt {checkAttempts + 1}
                      </span>
                    </div>
                    <Progress value={loadingProgress} className="h-2" />
                    <div className="flex items-center justify-center gap-2 text-xs text-muted-foreground">
                      <Loader2 className="h-3 w-3 animate-spin" />
                      <span>
                        {backendState === 'checking' && 'Checking server status...'}
                        {backendState === 'loading' && 'Waiting for server to start...'}
                      </span>
                    </div>
                  </div>

                  {/* Service Status Preview */}
                  {serviceStatuses.length > 0 && (
                    <div className="space-y-2 pt-2 border-t">
                      <p className="text-xs font-medium text-muted-foreground">Service Status:</p>
                      {serviceStatuses.map((service) => (
                        <div
                          key={service.id}
                          className="flex items-center justify-between p-2 rounded-md bg-background/50 border border-border"
                        >
                          <div className="flex items-center gap-2">
                            {getServiceStatusIcon(service.status)}
                            <span className="text-sm font-medium">{service.name}</span>
                          </div>
                          <span className={cn(
                            "text-xs px-2 py-1 rounded",
                            service.status === 'running' && "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400",
                            service.status === 'starting' && "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
                            service.status === 'error' && "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
                            service.status === 'stopped' && "bg-gray-100 text-gray-700 dark:bg-gray-900/30 dark:text-gray-400"
                          )}>
                            {service.status}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}
                </>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    );
  }

  // Backend is ready - show full login form
  return (
    <div className="min-h-screen flex items-center justify-center p-6 bg-background">
      <div className="w-full max-w-2xl space-y-6">
        {/* Header */}
        <div className="text-center space-y-4">
          <div className="flex justify-center">
            <div className="flex items-center justify-center bg-primary text-primary-foreground p-3 rounded-lg">
              <Lock className="h-6 w-6" />
              <span className="font-medium ml-2">AdapterOS</span>
            </div>
          </div>
          <div className="space-y-2">
            <h1 className="font-medium text-xl">Control Plane Access</h1>
            <p className="text-muted-foreground text-sm">
              Secure, air-gapped system management
            </p>
          </div>
        </div>

        {/* Time Ticker */}
        <div className="flex items-center justify-center space-x-2 text-muted-foreground">
          <Clock className="h-4 w-4" />
          <span className="font-mono text-sm">{formatTime(currentTime)}</span>
        </div>

        {/* Service Status Section - Live Updates */}
        <Card className="bg-muted/30">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base flex items-center gap-2">
                <Server className="h-4 w-4" />
                Service Status
              </CardTitle>
              <span className="text-xs text-muted-foreground">
                {runningServices}/{totalServices} running
              </span>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            {serviceStatuses.length > 0 ? (
              <>
                <div className="space-y-2">
                  {serviceStatuses.map((service) => (
                    <div
                      key={service.id}
                      className="flex items-center justify-between p-2 rounded-md bg-background/50 border border-border"
                    >
                      <div className="flex items-center gap-2">
                        {getServiceStatusIcon(service.status)}
                        <span className="text-sm font-medium">{service.name}</span>
                      </div>
                      <span className={cn(
                        "text-xs px-2 py-1 rounded",
                        service.status === 'running' && "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400",
                        service.status === 'starting' && "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
                        service.status === 'error' && "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
                        service.status === 'stopped' && "bg-gray-100 text-gray-700 dark:bg-gray-900/30 dark:text-gray-400"
                      )}>
                        {service.status}
                      </span>
                    </div>
                  ))}
                </div>
                {totalServices > 0 && (
                  <div className="space-y-1">
                    <div className="flex items-center justify-between text-xs text-muted-foreground">
                      <span>System Health</span>
                      <span>{Math.round(serviceProgress)}%</span>
                    </div>
                    <Progress value={serviceProgress} className="h-1.5" />
                  </div>
                )}
              </>
            ) : (
              <div className="text-sm text-muted-foreground text-center py-2">
                <Loader2 className="h-4 w-4 animate-spin mx-auto mb-2" />
                Loading service information...
              </div>
            )}
          </CardContent>
        </Card>

        {/* Security Indicators */}
        <div className="flex items-center justify-center space-x-3 flex-wrap gap-2">
          <div className="flex items-center space-x-2 px-3 py-1 bg-green-100 text-green-800 rounded-full text-xs dark:bg-green-900/30 dark:text-green-400">
            <Shield className="h-3 w-3" />
            Zero Egress
          </div>
          <div className="flex items-center space-x-2 px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-xs dark:bg-blue-900/30 dark:text-blue-400">
            <Lock className="h-3 w-3" />
            CSP Enforced
          </div>
          <div className="flex items-center space-x-2 px-3 py-1 bg-yellow-100 text-yellow-800 rounded-full text-xs dark:bg-yellow-900/30 dark:text-yellow-400">
            <AlertTriangle className="h-3 w-3" />
            ITAR Compliance Active
          </div>
        </div>

        {/* Login Form */}
        <Card>
          <CardHeader>
            <CardTitle>Authentication Required</CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
              {/* Error Display */}
              {error && (
                <Alert variant="destructive" className="border-red-500 bg-red-50 dark:bg-red-900/20">
                  <XCircle className="h-4 w-4" />
                  <AlertDescription className="text-sm font-medium">{error}</AlertDescription>
                </Alert>
              )}

              {/* Email Field */}
              <div className="space-y-2">
                <Label htmlFor="email" className="text-sm font-medium">
                  Email
                </Label>
                <Input
                  id="email"
                  type="email"
                  placeholder="Enter your email"
                  {...register('email')}
                  className={cn(
                    "border-2 transition-colors",
                    errors.email 
                      ? "border-red-500 focus-visible:border-red-500 focus-visible:ring-red-500/20" 
                      : "border-border focus-visible:border-primary"
                  )}
                  disabled={isLoading || isDevBypassLoading}
                />
                {errors.email && (
                  <p className="text-sm text-red-500 flex items-center gap-1">
                    <XCircle className="h-3 w-3" />
                    {errors.email.message}
                  </p>
                )}
              </div>

              {/* Password Field */}
              <div className="space-y-2">
                <Label htmlFor="password" className="text-sm font-medium">
                  Password
                </Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter your password"
                  {...register('password')}
                  className={cn(
                    "border-2 transition-colors",
                    errors.password 
                      ? "border-red-500 focus-visible:border-red-500 focus-visible:ring-red-500/20" 
                      : "border-border focus-visible:border-primary"
                  )}
                  disabled={isLoading || isDevBypassLoading}
                />
                {errors.password && (
                  <p className="text-sm text-red-500 flex items-center gap-1">
                    <XCircle className="h-3 w-3" />
                    {errors.password.message}
                  </p>
                )}
              </div>

              {/* Loading Progress */}
              {(isLoading || isDevBypassLoading) && (
                <div className="space-y-2">
                  <Progress value={undefined} className="h-1" />
                  <p className="text-xs text-muted-foreground text-center">
                    {isLoading ? 'Authenticating...' : 'Activating dev bypass...'}
                  </p>
                </div>
              )}

              {/* Action Buttons */}
              <div className="space-y-3 pt-2">
                <Button
                  type="submit"
                  className="w-full"
                  disabled={isLoading || isDevBypassLoading || !isValid || !watchedFields.email?.trim() || !watchedFields.password?.trim()}
                >
                  {isLoading ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Authenticating...
                    </>
                  ) : (
                    'Secure Login'
                  )}
                </Button>

                {isDev && (
                  <>
                    <div className="relative">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t border-border" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="bg-card px-2 text-muted-foreground">Development</span>
                      </div>
                    </div>
                    <Button
                      type="button"
                      variant="outline"
                      className="w-full"
                      onClick={handleDevBypass}
                      disabled={isDevBypassLoading || isLoading}
                    >
                      {isDevBypassLoading ? (
                        <>
                          <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                          Activating...
                        </>
                      ) : (
                        <>
                          <Zap className="h-4 w-4 mr-2" />
                          Dev Bypass (No Auth Required)
                        </>
                      )}
                    </Button>
                    <p className="text-xs text-muted-foreground text-center">
                      Development mode only - bypasses authentication
                    </p>
                  </>
                )}
              </div>
            </form>
          </CardContent>
        </Card>

        {/* Demo Credentials */}
        <Card className="bg-muted/50">
          <CardContent className="pt-6">
            <div className="text-sm space-y-2">
              <p className="font-medium text-muted-foreground">Demo Credentials:</p>
              <div className="space-y-2 text-xs">
                <div>
                  <p className="font-medium">Admin User:</p>
                  <p className="font-mono text-muted-foreground">
                    Email: admin@aos.local<br />
                    Password: password
                  </p>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
