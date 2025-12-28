import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Server, Shield, Database, Network } from 'lucide-react';

export default function DevOpsServerConfig() {
  return (
    <div className="space-y-4 h-full">
      <Tabs defaultValue="deployment" className="h-full">
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="deployment">Deployment</TabsTrigger>
          <TabsTrigger value="security">Security</TabsTrigger>
          <TabsTrigger value="resources">Resources</TabsTrigger>
        </TabsList>

        <TabsContent value="deployment" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center space-x-2">
                <Server className="h-4 w-4" />
                <span>Server Configuration</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Environment</Label>
                  <Select defaultValue="production">
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="development">Development</SelectItem>
                      <SelectItem value="staging">Staging</SelectItem>
                      <SelectItem value="production">Production</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div>
                  <Label>Server Mode</Label>
                  <Select defaultValue="uds">
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="http">HTTP (Port)</SelectItem>
                      <SelectItem value="uds">UDS Only</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div>
                <Label>UDS Socket Path</Label>
                <Input
                  placeholder="/var/run/adapteros.sock"
                  defaultValue="/var/run/adapteros.sock"
                />
              </div>

              <div className="flex items-center space-x-2">
                <Switch id="production-mode" defaultChecked />
                <Label htmlFor="production-mode">Production Mode</Label>
                <Badge variant="destructive">Required</Badge>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="security" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center space-x-2">
                <Shield className="h-4 w-4" />
                <span>Security Policies</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <Label>Egress Control</Label>
                    <p className="text-sm text-muted-foreground">Zero network egress in production</p>
                  </div>
                  <Switch defaultChecked />
                </div>

                <div className="flex items-center justify-between">
                  <div>
                    <Label>Workspace Isolation</Label>
                    <p className="text-sm text-muted-foreground">Multi-organization data separation</p>
                  </div>
                  <Switch defaultChecked />
                </div>

                <div className="flex items-center justify-between">
                  <div>
                    <Label>JWT Ed25519</Label>
                    <p className="text-sm text-muted-foreground">Production-grade authentication</p>
                  </div>
                  <Switch defaultChecked />
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="resources" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center space-x-2">
                <Database className="h-4 w-4" />
                <span>Resource Management</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label>Memory Limit (GB)</Label>
                  <Input type="number" defaultValue="32" />
                </div>
                <div>
                  <Label>Removal Threshold (%)</Label>
                  <Input type="number" defaultValue="85" />
                </div>
              </div>

              <div>
                <Label>Adapter Cache Strategy</Label>
                <Select defaultValue="lru">
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="lru">LRU (Least Recently Used)</SelectItem>
                    <SelectItem value="lfu">LFU (Least Frequently Used)</SelectItem>
                    <SelectItem value="ttl">TTL (Time To Live)</SelectItem>
                      </SelectContent>
                </Select>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      <div className="flex space-x-2">
        <Button className="flex-1">
          <Network className="h-4 w-4 mr-2" />
          Deploy Configuration
        </Button>
        <Button variant="outline">
          Validate Setup
        </Button>
      </div>
    </div>
  );
}
