import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Lock, Shield, AlertTriangle, XCircle } from 'lucide-react';
import { Alert, AlertDescription } from './ui/alert';

interface LoginFormProps {
  onLogin: (credentials: { email: string; password: string }) => Promise<void>;
  error?: string | null;
}

export function LoginForm({ onLogin, error }: LoginFormProps) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    
    try {
      await onLogin({ email, password });
    } catch (err) {
      // Error is handled by parent component
    } finally {
      setIsLoading(false);
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
            <form onSubmit={handleSubmit} className="space-y-4">
              {error && (
                <Alert variant="destructive">
                  <XCircle className="icon-standard" />
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              )}
              <div className="form-field">
                <Label htmlFor="email" className="form-label">Email</Label>
                <Input
                  id="email"
                  type="email"
                  placeholder="Enter your email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  required
                />
              </div>
              
              <div className="form-field">
                <Label htmlFor="password" className="form-label">Password</Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter your password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
              </div>
              
              <Button 
                type="submit" 
                className="w-full" 
                disabled={isLoading || !email || !password}
              >
                {isLoading ? 'Authenticating...' : 'Secure Login'}
              </Button>
            </form>
          </CardContent>
        </Card>

        {/* Demo Credentials */}
        <Card className="bg-muted/50">
          <CardContent className="pt-6">
            <div className="text-sm space-y-2">
              <p className="font-medium text-muted-foreground">Demo Credentials:</p>
              <div className="grid grid-cols-2 gap-4 text-xs">
                <div>
                  <p className="font-medium">Admin:</p>
                  <p>admin@aos.local / password</p>
                </div>
                <div>
                  <p className="font-medium">Operator:</p>
                  <p>operator@aos.local / password</p>
                </div>
                <div>
                  <p className="font-medium">SRE:</p>
                  <p>sre@aos.local / password</p>
                </div>
                <div>
                  <p className="font-medium">Viewer:</p>
                  <p>viewer@aos.local / password</p>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}