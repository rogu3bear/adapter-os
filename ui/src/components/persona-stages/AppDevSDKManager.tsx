import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Badge } from '../ui/badge';
import { Copy, Check, Eye, EyeOff, RefreshCw, Code2 } from 'lucide-react';

type Language = 'python' | 'typescript' | 'rust';

const SDK_VERSIONS: Record<Language, string> = {
  python: '0.3.2',
  typescript: '0.2.1',
  rust: '0.1.0',
};

const CODE_SNIPPETS: Record<Language, string> = {
  python: `from adapteros import AdapterOSClient

client = AdapterOSClient(
    api_key="YOUR_API_KEY",
    base_url="http://localhost:8080"
)

# Run inference
response = client.infer(
    prompt="Explain quantum computing",
    adapters=["my-adapter"],
    max_tokens=256
)

print(response.text)
print(f"Tokens: {response.tokens_generated}")
print(f"Latency: {response.latency_ms}ms")`,

  typescript: `import { AdapterOSClient } from '@adapteros/sdk';

const client = new AdapterOSClient({
  apiKey: 'YOUR_API_KEY',
  baseUrl: 'http://localhost:8080'
});

// Run inference
const response = await client.infer({
  prompt: 'Explain quantum computing',
  adapters: ['my-adapter'],
  maxTokens: 256
});

console.log(response.text);
console.log(\`Tokens: \${response.tokensGenerated}\`);
console.log(\`Latency: \${response.latencyMs}ms\`);`,

  rust: `use adapteros_client::AdapterOSClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AdapterOSClient::builder()
        .api_key("YOUR_API_KEY")
        .base_url("http://localhost:8080")
        .build()?;

    // Run inference
    let response = client.infer(InferRequest {
        prompt: "Explain quantum computing".into(),
        adapters: Some(vec!["my-adapter".into()]),
        max_tokens: Some(256),
        ..Default::default()
    }).await?;

    println!("{}", response.text);
    println!("Tokens: {}", response.tokens_generated);
    println!("Latency: {}ms", response.latency_ms);

    Ok(())
}`,
};

const INSTALL_COMMANDS: Record<Language, string> = {
  python: 'pip install adapteros',
  typescript: 'pnpm add @adapteros/sdk',
  rust: 'cargo add adapteros-client',
};

export default function AppDevSDKManager() {
  const [selectedLanguage, setSelectedLanguage] = useState<Language>('python');
  const [apiKey, setApiKey] = useState('aos_dev_key_xxxxxxxxxxxx');
  const [showApiKey, setShowApiKey] = useState(false);
  const [copiedCode, setCopiedCode] = useState(false);
  const [copiedInstall, setCopiedInstall] = useState(false);

  const copyToClipboard = async (text: string, type: 'code' | 'install') => {
    await navigator.clipboard.writeText(text);
    if (type === 'code') {
      setCopiedCode(true);
      setTimeout(() => setCopiedCode(false), 2000);
    } else {
      setCopiedInstall(true);
      setTimeout(() => setCopiedInstall(false), 2000);
    }
  };

  const regenerateApiKey = () => {
    const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
    const newKey = 'aos_dev_key_' + Array.from({ length: 24 }, () => chars[Math.floor(Math.random() * chars.length)]).join('');
    setApiKey(newKey);
  };

  return (
    <div className="space-y-4 p-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Code2 className="h-5 w-5" />
            SDK Configuration
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">API Key</label>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <Input
                  type={showApiKey ? 'text' : 'password'}
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  className="pr-10 font-mono text-sm"
                />
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="absolute right-1 top-1/2 -translate-y-1/2"
                  onClick={() => setShowApiKey(!showApiKey)}
                >
                  {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </Button>
              </div>
              <Button variant="outline" size="icon" onClick={regenerateApiKey}>
                <RefreshCw className="h-4 w-4" />
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Keep your API key secure. Never commit it to version control.
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">SDK Integration</CardTitle>
        </CardHeader>
        <CardContent>
          <Tabs value={selectedLanguage} onValueChange={(v) => setSelectedLanguage(v as Language)}>
            <TabsList className="mb-4">
              <TabsTrigger value="python">Python</TabsTrigger>
              <TabsTrigger value="typescript">TypeScript</TabsTrigger>
              <TabsTrigger value="rust">Rust</TabsTrigger>
            </TabsList>

            <TabsContent value={selectedLanguage} className="space-y-4">
              <div className="flex items-center justify-between">
                <Badge variant="secondary">
                  v{SDK_VERSIONS[selectedLanguage]}
                </Badge>
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">Install</span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => copyToClipboard(INSTALL_COMMANDS[selectedLanguage], 'install')}
                  >
                    {copiedInstall ? (
                      <Check className="h-4 w-4 text-green-500" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </Button>
                </div>
                <div className="rounded-md bg-muted p-3 font-mono text-sm">
                  {INSTALL_COMMANDS[selectedLanguage]}
                </div>
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">Quick Start</span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => copyToClipboard(CODE_SNIPPETS[selectedLanguage], 'code')}
                  >
                    {copiedCode ? (
                      <Check className="h-4 w-4 text-green-500" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </Button>
                </div>
                <div className="rounded-md bg-muted p-3 overflow-x-auto">
                  <pre className="text-sm">
                    <code>{CODE_SNIPPETS[selectedLanguage]}</code>
                  </pre>
                </div>
              </div>
            </TabsContent>
          </Tabs>
        </CardContent>
      </Card>
    </div>
  );
}
