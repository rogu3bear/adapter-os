# Z-Index Hierarchy Documentation

This document outlines the z-index organization for all overlay components in the UI system.

## Z-Index Levels

### z-10: Base Layout Components
- **Sidebar**: `z-10` - Base sidebar navigation component

### z-20: Dialog Components
- **DialogOverlay**: `z-20` - Backdrop overlay for dialogs
- **DialogContent**: `z-20` - Dialog content (rendered above overlay)

### z-30: Drawer Components
- **DrawerOverlay**: `z-30` - Backdrop overlay for drawers
- **DrawerContent**: `z-30` - Drawer content (rendered above overlay)

### z-40: Toast Notifications
- **Sonner Toaster**: `z-40` - Toast notification container

### z-50: High-Priority Overlays
All floating menu and popup components use `z-50` to ensure they appear above dialogs and drawers:

- **DropdownMenuContent**: `z-50` - Dropdown menu content
- **DropdownMenuSubContent**: `z-50` - Dropdown submenu content
- **SelectContent**: `z-50` - Select dropdown content
- **SheetOverlay**: `z-50` - Sheet backdrop overlay
- **SheetContent**: `z-50` - Sheet content
- **MenubarContent**: `z-50` - Menubar menu content
- **MenubarSubContent**: `z-50` - Menubar submenu content
- **PopoverContent**: `z-50` - Popover content
- **ContextMenuContent**: `z-50` - Context menu content
- **ContextMenuSubContent**: `z-50` - Context submenu content
- **AlertDialogOverlay**: `z-50` - Alert dialog backdrop overlay
- **AlertDialogContent**: `z-50` - Alert dialog content
- **HoverCardContent**: `z-50` - Hover card content
- **TooltipContent**: `z-50` - Tooltip content

## Frost Styling

All overlay components now use consistent frost glass styling:

- **Menu/Select Components**: `bg-popover/95 backdrop-blur-md`
- **Dialog/Sheet Components**: `bg-background/95 backdrop-blur-md`
- **Overlay Backdrops**: `bg-black/50 backdrop-blur-sm`

## Rationale

The z-index hierarchy ensures:
1. Dialogs and drawers (z-20/z-30) remain accessible but don't block high-priority menus
2. All menu overlays (z-50) appear consistently above all other content
3. Toast notifications (z-40) are visible but don't interfere with menus
4. Consistent visual styling with frost glass effect across all overlays

