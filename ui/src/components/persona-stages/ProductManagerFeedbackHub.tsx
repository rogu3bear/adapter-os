import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Textarea } from '../ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Checkbox } from '../ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import {
  Download,
  MessageSquare,
  ThumbsUp,
  ThumbsDown,
  Minus,
  MoreHorizontal,
  Reply,
  Archive,
  Flag,
  Search,
  Filter,
  Mail,
  Calendar,
  Tag,
} from 'lucide-react';

interface Feedback {
  id: string;
  userId: string;
  userEmail: string;
  category: 'bug' | 'feature' | 'improvement' | 'question' | 'other';
  sentiment: 'positive' | 'negative' | 'neutral';
  subject: string;
  message: string;
  status: 'new' | 'in-review' | 'responded' | 'resolved' | 'archived';
  priority: 'low' | 'medium' | 'high';
  createdAt: string;
  responseCount: number;
  tags: string[];
}

const initialFeedback: Feedback[] = [
  {
    id: 'fb-1',
    userId: 'user-123',
    userEmail: 'alice@example.com',
    category: 'feature',
    sentiment: 'positive',
    subject: 'Love the new dashboard!',
    message: 'The new dashboard layout is fantastic. Would be great to have customizable widgets.',
    status: 'in-review',
    priority: 'medium',
    createdAt: '2025-01-20T10:30:00Z',
    responseCount: 1,
    tags: ['dashboard', 'customization'],
  },
  {
    id: 'fb-2',
    userId: 'user-456',
    userEmail: 'bob@company.com',
    category: 'bug',
    sentiment: 'negative',
    subject: 'Adapter loading timeout',
    message: 'Getting frequent timeouts when loading large adapters. This is blocking our production deployment.',
    status: 'new',
    priority: 'high',
    createdAt: '2025-01-20T09:15:00Z',
    responseCount: 0,
    tags: ['adapter', 'performance', 'blocking'],
  },
  {
    id: 'fb-3',
    userId: 'user-789',
    userEmail: 'carol@startup.io',
    category: 'question',
    sentiment: 'neutral',
    subject: 'How to set up batch inference?',
    message: 'Could you provide documentation or examples for setting up batch inference with multiple adapters?',
    status: 'responded',
    priority: 'low',
    createdAt: '2025-01-19T16:45:00Z',
    responseCount: 2,
    tags: ['documentation', 'batch-inference'],
  },
  {
    id: 'fb-4',
    userId: 'user-101',
    userEmail: 'david@enterprise.com',
    category: 'improvement',
    sentiment: 'positive',
    subject: 'SSO integration request',
    message: 'Our organization requires SSO integration with Okta. This would help us adopt AdapterOS across teams.',
    status: 'in-review',
    priority: 'high',
    createdAt: '2025-01-19T14:20:00Z',
    responseCount: 3,
    tags: ['enterprise', 'sso', 'security'],
  },
  {
    id: 'fb-5',
    userId: 'user-202',
    userEmail: 'eve@ml-team.com',
    category: 'feature',
    sentiment: 'neutral',
    subject: 'Request: Model versioning',
    message: 'Would be helpful to have built-in model versioning with rollback capabilities.',
    status: 'new',
    priority: 'medium',
    createdAt: '2025-01-18T11:00:00Z',
    responseCount: 0,
    tags: ['versioning', 'mlops'],
  },
  {
    id: 'fb-6',
    userId: 'user-303',
    userEmail: 'frank@devshop.com',
    category: 'other',
    sentiment: 'positive',
    subject: 'Great support experience',
    message: 'Just wanted to say thanks for the quick response on my last ticket. Your support team is excellent!',
    status: 'resolved',
    priority: 'low',
    createdAt: '2025-01-17T09:30:00Z',
    responseCount: 1,
    tags: ['support', 'praise'],
  },
];

export default function ProductManagerFeedbackHub() {
  const [feedback, setFeedback] = useState<Feedback[]>(initialFeedback);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [sentimentFilter, setSentimentFilter] = useState<string>('all');
  const [responseDialogOpen, setResponseDialogOpen] = useState(false);
  const [selectedFeedback, setSelectedFeedback] = useState<Feedback | null>(null);
  const [responseText, setResponseText] = useState('');

  const filteredFeedback = feedback.filter((item) => {
    const matchesSearch =
      item.subject.toLowerCase().includes(searchQuery.toLowerCase()) ||
      item.message.toLowerCase().includes(searchQuery.toLowerCase()) ||
      item.userEmail.toLowerCase().includes(searchQuery.toLowerCase());
    const matchesStatus = statusFilter === 'all' || item.status === statusFilter;
    const matchesSentiment = sentimentFilter === 'all' || item.sentiment === sentimentFilter;
    return matchesSearch && matchesStatus && matchesSentiment;
  });

  const toggleSelectAll = () => {
    if (selectedIds.length === filteredFeedback.length) {
      setSelectedIds([]);
    } else {
      setSelectedIds(filteredFeedback.map((f) => f.id));
    }
  };

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) =>
      prev.includes(id) ? prev.filter((i) => i !== id) : [...prev, id]
    );
  };

  const getSentimentIcon = (sentiment: string) => {
    switch (sentiment) {
      case 'positive':
        return <ThumbsUp className="h-4 w-4 text-green-500" />;
      case 'negative':
        return <ThumbsDown className="h-4 w-4 text-red-500" />;
      default:
        return <Minus className="h-4 w-4 text-gray-500" />;
    }
  };

  const getSentimentBadgeVariant = (sentiment: string) => {
    switch (sentiment) {
      case 'positive':
        return 'default';
      case 'negative':
        return 'destructive';
      default:
        return 'secondary';
    }
  };

  const getStatusBadgeVariant = (status: string) => {
    switch (status) {
      case 'new':
        return 'default';
      case 'in-review':
        return 'secondary';
      case 'responded':
        return 'outline';
      case 'resolved':
        return 'default';
      default:
        return 'outline';
    }
  };

  const getCategoryBadgeVariant = (category: string) => {
    switch (category) {
      case 'bug':
        return 'destructive';
      case 'feature':
        return 'default';
      case 'improvement':
        return 'secondary';
      default:
        return 'outline';
    }
  };

  const getPriorityColor = (priority: string) => {
    switch (priority) {
      case 'high':
        return 'text-red-500';
      case 'medium':
        return 'text-yellow-500';
      default:
        return 'text-gray-500';
    }
  };

  const handleResponse = (item: Feedback) => {
    setSelectedFeedback(item);
    setResponseDialogOpen(true);
  };

  const sendResponse = () => {
    if (!selectedFeedback || !responseText.trim()) return;
    setFeedback((prev) =>
      prev.map((f) =>
        f.id === selectedFeedback.id
          ? { ...f, status: 'responded' as const, responseCount: f.responseCount + 1 }
          : f
      )
    );
    setResponseText('');
    setResponseDialogOpen(false);
    setSelectedFeedback(null);
  };

  const archiveSelected = () => {
    setFeedback((prev) =>
      prev.map((f) =>
        selectedIds.includes(f.id) ? { ...f, status: 'archived' as const } : f
      )
    );
    setSelectedIds([]);
  };

  const exportToCSV = () => {
    const headers = [
      'ID',
      'User Email',
      'Category',
      'Sentiment',
      'Subject',
      'Message',
      'Status',
      'Priority',
      'Created At',
      'Response Count',
      'Tags',
    ];
    const rows = filteredFeedback.map((f) => [
      f.id,
      f.userEmail,
      f.category,
      f.sentiment,
      `"${f.subject.replace(/"/g, '""')}"`,
      `"${f.message.replace(/"/g, '""')}"`,
      f.status,
      f.priority,
      f.createdAt,
      f.responseCount.toString(),
      f.tags.join(';'),
    ]);
    const csv = [headers.join(','), ...rows.map((r) => r.join(','))].join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `feedback-export-${new Date().toISOString().split('T')[0]}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const formatDate = (dateString: string) => {
    const date = new Date(dateString);
    return date.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const sentimentCounts = {
    positive: feedback.filter((f) => f.sentiment === 'positive').length,
    negative: feedback.filter((f) => f.sentiment === 'negative').length,
    neutral: feedback.filter((f) => f.sentiment === 'neutral').length,
  };

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Feedback Hub</h1>
          <p className="text-sm text-muted-foreground">
            Manage and respond to user feedback
          </p>
        </div>
        <Button onClick={exportToCSV} variant="outline" className="gap-2">
          <Download className="h-4 w-4" />
          Export CSV
        </Button>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Total Feedback</p>
                <p className="text-2xl font-bold">{feedback.length}</p>
              </div>
              <MessageSquare className="h-8 w-8 text-muted-foreground" />
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Positive</p>
                <p className="text-2xl font-bold text-green-500">{sentimentCounts.positive}</p>
              </div>
              <ThumbsUp className="h-8 w-8 text-green-500" />
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Negative</p>
                <p className="text-2xl font-bold text-red-500">{sentimentCounts.negative}</p>
              </div>
              <ThumbsDown className="h-8 w-8 text-red-500" />
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Pending Response</p>
                <p className="text-2xl font-bold">
                  {feedback.filter((f) => f.status === 'new').length}
                </p>
              </div>
              <Mail className="h-8 w-8 text-muted-foreground" />
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Filters */}
      <Card>
        <CardContent className="p-4">
          <div className="flex flex-wrap items-center gap-4">
            <div className="flex-1 min-w-[200px]">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder="Search feedback..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-9"
                />
              </div>
            </div>
            <Select value={statusFilter} onValueChange={setStatusFilter}>
              <SelectTrigger className="w-[150px]">
                <Filter className="h-4 w-4 mr-2" />
                <SelectValue placeholder="Status" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Status</SelectItem>
                <SelectItem value="new">New</SelectItem>
                <SelectItem value="in-review">In Review</SelectItem>
                <SelectItem value="responded">Responded</SelectItem>
                <SelectItem value="resolved">Resolved</SelectItem>
                <SelectItem value="archived">Archived</SelectItem>
              </SelectContent>
            </Select>
            <Select value={sentimentFilter} onValueChange={setSentimentFilter}>
              <SelectTrigger className="w-[150px]">
                <SelectValue placeholder="Sentiment" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Sentiment</SelectItem>
                <SelectItem value="positive">Positive</SelectItem>
                <SelectItem value="neutral">Neutral</SelectItem>
                <SelectItem value="negative">Negative</SelectItem>
              </SelectContent>
            </Select>
            {selectedIds.length > 0 && (
              <Button variant="outline" onClick={archiveSelected} className="gap-2">
                <Archive className="h-4 w-4" />
                Archive ({selectedIds.length})
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Feedback Table */}
      <Card>
        <CardHeader>
          <CardTitle>Feedback List</CardTitle>
          <CardDescription>
            {filteredFeedback.length} items
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[40px]">
                  <Checkbox
                    checked={selectedIds.length === filteredFeedback.length && filteredFeedback.length > 0}
                    onCheckedChange={toggleSelectAll}
                  />
                </TableHead>
                <TableHead>Sentiment</TableHead>
                <TableHead>Subject</TableHead>
                <TableHead>Category</TableHead>
                <TableHead>User</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Priority</TableHead>
                <TableHead>Date</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredFeedback.map((item) => (
                <TableRow key={item.id}>
                  <TableCell>
                    <Checkbox
                      checked={selectedIds.includes(item.id)}
                      onCheckedChange={() => toggleSelect(item.id)}
                    />
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      {getSentimentIcon(item.sentiment)}
                      <Badge variant={getSentimentBadgeVariant(item.sentiment)} className="text-xs">
                        {item.sentiment}
                      </Badge>
                    </div>
                  </TableCell>
                  <TableCell>
                    <div>
                      <div className="font-medium">{item.subject}</div>
                      <div className="text-xs text-muted-foreground line-clamp-1">
                        {item.message}
                      </div>
                      {item.tags.length > 0 && (
                        <div className="flex items-center gap-1 mt-1">
                          <Tag className="h-3 w-3 text-muted-foreground" />
                          {item.tags.slice(0, 2).map((tag) => (
                            <Badge key={tag} variant="outline" className="text-xs py-0">
                              {tag}
                            </Badge>
                          ))}
                          {item.tags.length > 2 && (
                            <span className="text-xs text-muted-foreground">
                              +{item.tags.length - 2}
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  </TableCell>
                  <TableCell>
                    <Badge variant={getCategoryBadgeVariant(item.category)}>
                      {item.category}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="text-sm">{item.userEmail}</div>
                  </TableCell>
                  <TableCell>
                    <Badge variant={getStatusBadgeVariant(item.status)}>
                      {item.status.replace('-', ' ')}
                    </Badge>
                    {item.responseCount > 0 && (
                      <div className="text-xs text-muted-foreground mt-1">
                        {item.responseCount} response{item.responseCount > 1 ? 's' : ''}
                      </div>
                    )}
                  </TableCell>
                  <TableCell>
                    <span className={`text-sm font-medium ${getPriorityColor(item.priority)}`}>
                      {item.priority}
                    </span>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1 text-sm text-muted-foreground">
                      <Calendar className="h-3 w-3" />
                      {formatDate(item.createdAt)}
                    </div>
                  </TableCell>
                  <TableCell className="text-right">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="icon">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => handleResponse(item)}>
                          <Reply className="h-4 w-4 mr-2" />
                          Respond
                        </DropdownMenuItem>
                        <DropdownMenuItem>
                          <Flag className="h-4 w-4 mr-2" />
                          Flag
                        </DropdownMenuItem>
                        <DropdownMenuItem>
                          <Archive className="h-4 w-4 mr-2" />
                          Archive
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Response Dialog */}
      <Dialog open={responseDialogOpen} onOpenChange={setResponseDialogOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>Respond to Feedback</DialogTitle>
            <DialogDescription>
              {selectedFeedback?.userEmail} - {selectedFeedback?.subject}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="p-3 bg-muted rounded-md">
              <p className="text-sm">{selectedFeedback?.message}</p>
            </div>
            <Textarea
              placeholder="Type your response..."
              value={responseText}
              onChange={(e) => setResponseText(e.target.value)}
              rows={5}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setResponseDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={sendResponse} disabled={!responseText.trim()}>
              Send Response
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
