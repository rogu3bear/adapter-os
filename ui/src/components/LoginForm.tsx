import React, { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Lock, Shield, AlertTriangle, XCircle, Zap } from 'lucide-react';
import { Alert, AlertDescription } from './ui/alert';
import { apiClient } from '../api/client';
import { LoginFormSchema, type LoginFormData } from '../schemas/common.schema';

interface LoginFormProps {
  onLogin: (credentials: { username: string; email: string; password: string }) => Promise<void>;
  onDevBypass?: () => Promise<void>;
  error?: string | null;
}

export function LoginForm({ onLogin, onDevBypass, error }: LoginFormProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [isDevBypassLoading, setIsDevBypassLoading] = useState(false);
  const isDev = import.meta.env.DEV;

  const {
    register,
    handleSubmit,
    formState: { errors, isValid },
    watch,
  } = useForm<LoginFormData>({
    resolver: zodResolver(LoginFormSchema),
    mode: 'onChange',
    defaultValues: {
      username: '',
      email: '',
      password: '',
    },
  });

  const watchedFields = watch();

  const onSubmit = async (data: LoginFormData) => {
    setIsLoading(true);

    try {
      await onLogin({
        username: data.username.trim(),
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
      await apiClient.devBypass();
      // Call onDevBypass callback to update auth state
      if (onDevBypass) {
        await onDevBypass();
      }
    } catch (err) {
      // Error will be shown via error prop
    } finally {
      setIsDevBypassLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center p-6 bg-background">
      <div className="w-full max-w-md space-y-6">
        {/* Header */}
        <div className="text-center space-y-4">
          <div className="flex justify-center">
            <div className="flex items-center justify-center bg-primary text-primary-foreground p-3 rounded-lg">
              <Lock className="h-6 w-6" />
              <span className="font-medium">AdapterOS</span>
            </div>
          </div>
          <div className="space-y-2">
            <h1 className="font-medium">Control Plane Access</h1>
            <p className="text-muted-foreground">
              Secure, air-gapped system management
            </p>
          </div>
        </div>

        {/* Security Indicators */}
        <div className="flex items-center justify-center space-x-3">
          <div className="flex items-center space-x-2 px-3 py-1 bg-green-100 text-green-800 rounded-full text-sm">
            <Shield className="h-4 w-4" />
            Zero Egress
          </div>
          <div className="flex items-center space-x-2 px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-sm">
            <Lock className="h-4 w-4" />
            CSP Enforced
          </div>
          <div className="flex items-center space-x-2 px-3 py-1 bg-yellow-100 text-yellow-800 rounded-full text-sm">
            <AlertTriangle className="h-4 w-4" />
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
              {error && (
                <Alert variant="destructive">
                  <XCircle className="icon-standard" />
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
              <div className="mb-4">
                <Label htmlFor="username" className="font-medium text-sm mb-1">Username</Label>
                <Input
                  id="username"
                  type="text"
                  placeholder="Enter your username"
                  {...register('username')}
                  className={errors.username ? 'border-red-500' : ''}
                />
                {errors.username && (
                  <p className="text-sm text-red-500 mt-1">{errors.username.message}</p>
                )}
              </div>

              <div className="mb-4">
                <Label htmlFor="email" className="font-medium text-sm mb-1">Email</Label>
                <Input
                  id="email"
                  type="email"
                  placeholder="Enter your email"
                  {...register('email')}
                  className={errors.email ? 'border-red-500' : ''}
                />
                {errors.email && (
                  <p className="text-sm text-red-500 mt-1">{errors.email.message}</p>
                )}
              </div>

              <div className="mb-4">
                <Label htmlFor="password" className="font-medium text-sm mb-1">Password</Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter your password"
                  {...register('password')}
                  className={errors.password ? 'border-red-500' : ''}
                />
                {errors.password && (
                  <p className="text-sm text-red-500 mt-1">{errors.password.message}</p>
                )}
              </div>

              <Button
                type="submit"
                className="w-full"
                disabled={isLoading || !isValid || !watchedFields.username?.trim() || !watchedFields.email?.trim() || !watchedFields.password?.trim()}
              >
                {isLoading ? 'Authenticating...' : 'Secure Login'}
              </Button>

              {isDev && (
                <div className="pt-2 border-t">
                  <Button
                    type="button"
                    variant="outline"
                    className="w-full"
                    onClick={handleDevBypass}
                    disabled={isDevBypassLoading || isLoading}
                  >
                    <Zap className="h-4 w-4 mr-2" />
                    {isDevBypassLoading ? 'Activating...' : 'Dev Bypass (No Auth Required)'}
                  </Button>
                  <p className="text-xs text-muted-foreground mt-2 text-center">
                    Development mode only - bypasses authentication
                  </p>
                </div>
              )}
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
                    Username: admin<br />
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