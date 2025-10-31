/**
 * ContactsPage Component
 * 
 * Displays contacts discovered during inference with real-time updates via SSE.
 * Categories: User | System | Adapter | Repository | External
 * 
 * Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §8.1
 */

import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { Contact } from '@/api/types';

interface ContactsPageProps {
  selectedTenant: string;
}

export function ContactsPage({ selectedTenant }: ContactsPageProps) {
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [filter, setFilter] = useState<string>('all');
  const [searchTerm, setSearchTerm] = useState<string>('');
  const [loading, setLoading] = useState(true);

  // Fetch initial contacts
  useEffect(() => {
    fetchContacts();
  }, [fetchContacts, filter]);

  // Subscribe to contact updates via SSE
  useEffect(() => {
    const eventSource = new EventSource(
      `/api/v1/streams/contacts?tenant=${selectedTenant}`
    );

    eventSource.addEventListener('contact', (event) => {
      const data = JSON.parse(event.data);
      if (data.type === 'contact_discovered') {
        // Add or update contact in real-time
        setContacts((prev) => {
          const existing = prev.find((c) => c.name === data.payload.name);
          if (existing) {
            return prev.map((c) =>
              c.name === data.payload.name
                ? { ...c, ...data.payload, interaction_count: c.interaction_count + 1 }
                : c
            );
          } else {
            return [...prev, { id: data.payload.name, ...data.payload, interaction_count: 1 }];
          }
        });
      }
    });

    return () => eventSource.close();
  }, [selectedTenant]);

  const fetchContacts = useCallback(async () => {
    setLoading(true);
    try {
      // Citation: ui/src/api/client.ts L57-L105
      const data = await apiClient.listContacts(selectedTenant);
      setContacts(data);
    } catch (error) {
      logger.error('Failed to fetch contacts', {
        component: 'ContactsPage',
        operation: 'listContacts',
        tenantId: selectedTenant,
      }, toError(error));
    } finally {
      setLoading(false);
    }
  }, [selectedTenant]);

  const filteredContacts = contacts.filter(
    (c) =>
      c.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      c.email?.toLowerCase().includes(searchTerm.toLowerCase())
  );

  const getCategoryColor = (category: string) => {
    switch (category) {
      case 'user':
        return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-300';
      case 'system':
        return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-300';
      case 'adapter':
        return 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-300';
      case 'repository':
        return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300';
      case 'external':
        return 'bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-300';
      default:
        return 'bg-gray-100 text-gray-800';
    }
  };

  const categoryCount = (category: string) =>
    contacts.filter((c) => c.category === category).length;

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold">Contacts</h1>
          <p className="text-gray-600 dark:text-gray-400 mt-2">
            Discovered during inference • {contacts.length} total
          </p>
        </div>
        <Button onClick={fetchContacts}>Refresh</Button>
      </div>

      {/* Filters */}
      <div className="flex gap-4">
        <Input
          placeholder="Search contacts..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="max-w-sm"
        />

        <Select value={filter} onValueChange={setFilter}>
          <SelectTrigger className="w-[180px]">
            <SelectValue placeholder="All Categories" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Categories</SelectItem>
            <SelectItem value="user">Users</SelectItem>
            <SelectItem value="system">System</SelectItem>
            <SelectItem value="adapter">Adapters</SelectItem>
            <SelectItem value="repository">Repositories</SelectItem>
            <SelectItem value="external">External</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Category Summary */}
      <div className="grid grid-cols-5 gap-4">
        {['user', 'system', 'adapter', 'repository', 'external'].map((category) => (
          <Card key={category}>
            <CardHeader>
              <CardTitle className="text-sm font-medium capitalize">{category}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{categoryCount(category)}</div>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Contact List */}
      {loading ? (
        <div className="text-center py-12">Loading...</div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredContacts.map((contact) => (
            <Card key={contact.id} className="hover:shadow-lg transition-shadow">
              <CardHeader>
                <div className="flex justify-between items-start">
                  <div>
                    <CardTitle>{contact.name}</CardTitle>
                    <CardDescription>{contact.email}</CardDescription>
                  </div>
                  <Badge className={getCategoryColor(contact.category)}>{contact.category}</Badge>
                </div>
              </CardHeader>
              <CardContent>
                <div className="space-y-2 text-sm">
                  {contact.role && (
                    <div>
                      <span className="font-medium">Role:</span> {contact.role}
                    </div>
                  )}
                  <div>
                    <span className="font-medium">Interactions:</span> {contact.interaction_count}
                  </div>
                  <div className="text-gray-500 dark:text-gray-400">
                    Discovered {new Date(contact.discovered_at).toLocaleDateString()}
                  </div>
                  {contact.last_interaction && (
                    <div className="text-gray-500 dark:text-gray-400">
                      Last active {new Date(contact.last_interaction).toLocaleDateString()}
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
