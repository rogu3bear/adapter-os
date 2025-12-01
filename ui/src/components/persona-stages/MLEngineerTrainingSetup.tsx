import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Terminal, Play, Settings, FileText } from 'lucide-react';

export default function MLEngineerTrainingSetup() {
  return (
    <div className="space-y-4 h-full">
      {/* Terminal Header */}
      <div className="flex items-center space-x-2 p-3 bg-muted rounded-lg">
        <Terminal className="h-4 w-4" />
        <span className="font-medium text-sm">AdapterOS Training CLI</span>
        <Badge variant="outline" className="ml-auto">v2.1.0</Badge>
      </div>

      {/* Training Configuration */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center space-x-2">
            <Settings className="h-4 w-4" />
            <span>Training Configuration</span>
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <Label htmlFor="dataset">Dataset Path</Label>
              <Input
                id="dataset"
                placeholder="./custom_data.jsonl"
                defaultValue="./custom_data.jsonl"
              />
            </div>
            <div>
              <Label htmlFor="base-model">Base Model</Label>
              <Select defaultValue="llama-2-7b">
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="llama-2-7b">Llama 2 7B</SelectItem>
                  <SelectItem value="llama-2-13b">Llama 2 13B</SelectItem>
                  <SelectItem value="codellama-7b">CodeLlama 7B</SelectItem>
                  <SelectItem value="mistral-7b">Mistral 7B</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="grid grid-cols-3 gap-4">
            <div>
              <Label htmlFor="output">Output Directory</Label>
              <Input
                id="output"
                placeholder="./adapters/my_adapter"
                defaultValue="./adapters/my_adapter"
              />
            </div>
            <div>
              <Label htmlFor="config">Config File</Label>
              <Input
                id="config"
                placeholder="training-config.toml"
                defaultValue="training-config.toml"
              />
            </div>
            <div>
              <Label htmlFor="gpu">GPU Count</Label>
              <Select defaultValue="1">
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="1">1 GPU</SelectItem>
                  <SelectItem value="2">2 GPUs</SelectItem>
                  <SelectItem value="4">4 GPUs</SelectItem>
                  <SelectItem value="8">8 GPUs</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Command Preview */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center space-x-2">
            <FileText className="h-4 w-4" />
            <span>Generated Command</span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="bg-slate-900 text-green-400 p-3 rounded font-mono text-sm overflow-x-auto">
            <div>$ aos train --dataset ./custom_data.jsonl \</div>
            <div className="ml-4">  --base-model llama-2-7b \</div>
            <div className="ml-4">  --output-dir ./adapters/my_adapter \</div>
            <div className="ml-4">  --config training-config.toml</div>
          </div>
        </CardContent>
      </Card>

      {/* Action Buttons */}
      <div className="flex space-x-2">
        <Button className="flex-1">
          <Play className="h-4 w-4 mr-2" />
          Start Training
        </Button>
        <Button variant="outline">
          Validate Config
        </Button>
      </div>
    </div>
  );
}
