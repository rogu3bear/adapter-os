# AdapterOS Persona Journey Demo - User Guide

**Interactive Exploration of AdapterOS from Every User Perspective**

---

## 🎯 Overview

The Persona Journey Demo is an interactive experience that showcases how AdapterOS presents different interfaces and capabilities to different types of users. Navigate through 6 distinct user personas, each with 4 workflow stages, to understand how the system adapts to different needs and contexts.

**🎚️ Key Feature**: Bottom slider navigation lets you seamlessly switch between user perspectives while maintaining context of their workflow stage.

---

## 🚀 Getting Started

### Prerequisites
- Node.js 18+
- pnpm package manager
- Modern web browser

### Installation & Setup

```bash
# Clone the repository
git clone https://github.com/your-org/adapter-os.git
cd adapter-os/ui

# Install dependencies
pnpm install

# Start development server
pnpm dev
```

### Accessing the Demo

1. Open your browser to `http://localhost:3200`
2. Navigate to **"Persona Demo"** in the left sidebar (under "Home")
3. The demo loads with the **ML Engineer** persona, showing their first workflow stage

---

## 🎨 Interface Overview

### Layout Structure

```
┌─────────────────────────────────────┐
│ Header: Persona + Stage Info       │
├─────────────────┬───────────────────┤
│                 │                   │
│  Stage Viewer   │  Info Panels      │
│  (Main Content) │  (What/Why/When)  │
│                 │                   │
├─────────────────┴───────────────────┤
│ 👥 Persona Slider (Bottom)          │
│ [ML Eng] [DevOps] [App Dev] [Sec]   │
└─────────────────────────────────────┘
```

### Navigation Elements

#### Bottom Persona Slider 🎚️
- **6 Persona Cards**: Click any card to switch user perspectives
- **Active Highlighting**: Current persona shows with primary color and "Active" badge
- **Responsive**: Cards scroll horizontally on smaller screens

#### Stage Navigation
- **Prev/Next Buttons**: Navigate through 4 stages per persona
- **Stage Indicator**: Shows current stage (e.g., "Stage 2 of 4")
- **Context Preservation**: Switching personas remembers your stage progress

#### Information Panels (Right Side)
- **What Appears**: UI/interface description
- **Why**: Purpose and business value
- **Context**: When/where in workflow this appears

---

## 👥 The Six Personas

### 1. 🎯 ML Engineer
**"Training and deploying custom LoRA adapters"**

**Stages:**
1. **Training Environment Setup** - CLI interface for adapter training
2. **Model Registry Interaction** - Browse and manage trained adapters
3. **Performance Monitoring** - Training metrics and GPU utilization
4. **Inference Testing** - Interactive prompt interface

### 2. 🔧 DevOps Engineer
**"Managing infrastructure and production deployments"**

**Stages:**
1. **Server Configuration** - Deployment profiles and security settings
2. **Resource Management** - Memory, GPU, and storage monitoring
3. **CI/CD Integration** - Automated deployment workflows
4. **System Monitoring** - Health dashboards and alerting

### 3. 💻 Application Developer
**"Integrating AI capabilities into applications"**

**Stages:**
1. **API Documentation** - Interactive docs with code examples
2. **SDK Management** - Client library downloads
3. **Integration Testing** - API testing and debugging
4. **Performance Optimization** - Latency and cost monitoring

### 4. 🔒 Security Engineer
**"Ensuring compliance and policy enforcement"**

**Stages:**
1. **Policy Configuration** - Security rule definition
2. **Audit Trail Review** - Event logging and compliance
3. **Isolation Testing** - Tenant separation verification
4. **Threat Detection** - Real-time security monitoring

### 5. 📊 Data Scientist
**"Experimenting with and evaluating adapters"**

**Stages:**
1. **Experiment Tracking** - A/B testing and comparison
2. **Dataset Preparation** - Data upload and preprocessing
3. **Evaluation Framework** - Benchmarking and metrics
4. **Collaboration Hub** - Team sharing and notebooks

### 6. 📈 Product Manager
**"Overseeing product strategy and requirements"**

**Stages:**
1. **Usage Analytics** - Feature adoption and metrics
2. **System Performance** - Business KPIs and uptime
3. **Configuration Management** - Service tiers and templates
4. **Feedback Integration** - User requirements and roadmapping

---

## 🎮 Interactive Features

### Stage Exploration
Each persona's journey includes **realistic mock interfaces** that demonstrate what they would actually see:

#### Fully Implemented Stages
- **ML Engineer Registry**: Searchable table with adapter metadata, status indicators, and management actions
- **DevOps Resource Dashboard**: Real-time metrics, CPU/GPU monitoring, alerts, and performance trends
- **Security Policy Editor**: Interactive policy configuration with enable/disable toggles and severity indicators
- **App Developer API Docs**: Tabbed interface with request/response examples and language selection

#### Preview Stages
- **21 additional stages** show contextual mockups representing their functionality
- Each includes relevant UI patterns and data structures for that workflow

### Navigation Patterns

#### Persona Switching
```
Current: ML Engineer (Stage 2/4)
Click "DevOps" → Switches to DevOps (Stage 1/4)
Click "ML Engineer" → Returns to ML Engineer (Stage 2/4)
```

#### Stage Progression
```
ML Engineer Journey:
Stage 1 ← [Current: Stage 2] → Stage 3
       ↑                        ↓
   End of Previous         Start of Next
   Persona Journey         Persona Journey
```

#### Information Context
- **What**: "Interactive prompt interface with adapter selection dropdown"
- **Why**: "Test adapter behavior before production deployment"
- **Context**: "Quality assurance stage"

---

## 🔧 Advanced Usage

### Keyboard Navigation
- **Tab**: Navigate between interactive elements
- **Enter/Space**: Activate buttons and switches
- **Arrow Keys**: Navigate persona slider (when focused)

### Responsive Design
- **Desktop**: Full 3-panel layout with side-by-side content
- **Tablet**: Stacked layout with collapsible panels
- **Mobile**: Single-column with bottom slider navigation

### Accessibility Features
- **Screen Reader Support**: All interactive elements labeled
- **High Contrast**: Clear visual hierarchy and color coding
- **Keyboard Only**: Full functionality without mouse
- **Focus Indicators**: Clear focus states for navigation

---

## 🎨 UI Patterns Demonstrated

### Data Visualization
- **Tables**: Sortable, filterable data grids (Registry Browser)
- **Charts**: Progress bars, trend indicators (Resource Dashboard)
- **Metrics Cards**: KPI displays with status indicators
- **Status Badges**: Color-coded state representation

### Interactive Controls
- **Toggles**: Policy enable/disable switches
- **Dropdowns**: Configuration and filter selections
- **Tabs**: Multi-section content organization
- **Search**: Real-time filtering and results

### Information Architecture
- **Progressive Disclosure**: Show/hide detailed information
- **Contextual Help**: Tooltips and inline explanations
- **Status Indicators**: Visual feedback for system states
- **Action Buttons**: Clear primary and secondary actions

---

## 🔍 Understanding AdapterOS Through Personas

### Core System Concepts Demonstrated

#### 1. **Multi-Tenant Architecture**
- Security Engineer's isolation testing
- DevOps tenant management
- Product Manager configuration templates

#### 2. **LoRA Adapter Lifecycle**
- ML Engineer's training and registry management
- Data Scientist's evaluation and experimentation
- DevOps deployment and monitoring

#### 3. **Policy Enforcement**
- Security Engineer's policy configuration
- System-wide compliance validation
- Audit trail and evidence collection

#### 4. **Performance Optimization**
- DevOps resource monitoring
- App Developer performance optimization
- Product Manager business metrics

#### 5. **API Integration**
- App Developer's SDK and documentation
- Multiple language support (JS, Python, Go, Rust)
- RESTful endpoint design

---

## 🐛 Troubleshooting

### Common Issues

#### Demo Won't Load
```bash
# Check if dev server is running
curl http://localhost:3200

# Restart development server
pnpm dev
```

#### Navigation Not Working
- Ensure JavaScript is enabled
- Try refreshing the page
- Check browser console for errors

#### Components Show "Mock Preview"
- These are intentionally simplified placeholders
- Fully implemented components include interactive features
- Check the "What Appears" panel for intended functionality

#### Mobile Layout Issues
- Demo is optimized for desktop viewing
- Use landscape orientation on mobile devices
- Bottom slider may require horizontal scrolling

---

## 📈 Demo Evolution

### Current State (v1.0)
- ✅ 6 personas with 4 stages each
- ✅ Interactive navigation and context switching
- ✅ 4 fully implemented realistic mock interfaces
- ✅ Responsive design and accessibility
- ✅ Comprehensive user documentation

### Future Enhancements (v2.0)
- **Live API Integration**: Connect to real AdapterOS backend
- **Interactive Mockups**: More stages with full functionality
- **Scenario Simulation**: Walk through complete workflows
- **Performance Metrics**: Real system monitoring integration
- **Multi-User Scenarios**: Collaboration and sharing features

---

## 🤝 Contributing to the Demo

### Adding New Personas
1. Add persona definition to `persona-journeys.ts`
2. Create 4 stage components in `persona-stages/`
3. Update PersonaSlider to include new persona
4. Test navigation and responsive behavior

### Improving Mock Components
1. Study real AdapterOS interfaces
2. Implement realistic data structures
3. Add interactive behaviors
4. Ensure accessibility compliance

### Documentation Updates
1. Update persona descriptions
2. Add new feature explanations
3. Include troubleshooting guides
4. Document API integration points

---

## 📚 Related Documentation

- **[Architecture Overview](../docs/architecture/)**
- **[Security Implementation](../docs/secure-enclave-integration.md)**
- **[API Documentation](../docs/api.md)**
- **[Deployment Guide](../docs/architecture/production_deployment.md)**

---

## 🎯 Key Takeaways

The Persona Journey Demo illustrates how **AdapterOS adapts to different users**:

- **ML Engineers**: Focus on training, registry, and performance
- **DevOps**: Emphasize infrastructure, monitoring, and reliability
- **App Developers**: Prioritize integration, documentation, and optimization
- **Security Engineers**: Center on policies, compliance, and threat detection
- **Data Scientists**: Highlight experimentation, evaluation, and collaboration
- **Product Managers**: Showcase analytics, performance, and user feedback

Each persona sees **relevant interfaces** at the **right time** in their workflow, demonstrating AdapterOS's **user-centric design philosophy**.

---

**Demo URL**: `/personas` (when running `pnpm dev`)  
**Source Code**: `ui/src/components/persona-stages/`  
**Last Updated**: 2025-01-15
