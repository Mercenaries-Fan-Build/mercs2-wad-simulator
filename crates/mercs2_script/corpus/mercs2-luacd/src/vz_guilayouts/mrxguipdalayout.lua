LocalWidgetList = {}
if import then
  import("MrxGuiPda")
  import("MrxGuiTextBuffer")
else
  MrxGuiPda = {}
  MrxGuiTextBuffer = {}
end
LocalWidgetList[1] = {
  name = "PDA",
  x1 = 0,
  y1 = 0,
  x2 = 640,
  y2 = 480,
  RedLevel = 255,
  GreenLevel = 255,
  BlueLevel = 255,
  TranslucencyLevel = 255,
  HorizontalAnchor = "center",
  VerticalAnchor = "center",
  visible = 1,
  container = false,
  EventHandlerFile = "MrxGuiPda",
  EventHandlers = {
    GuiInitialization = MrxGuiPda._Initialize,
    SetGPSDest = MrxGuiPda.HandleMarkerUpdate,
    ClearGPSDest = MrxGuiPda.HandleMarkerClear,
    TogglePDA = MrxGuiPda._HandleToggleEvent
  },
  EventHandlerNames = {
    GuiInitialization = "_Initialize",
    SetGPSDest = "HandleMarkerUpdate",
    ClearGPSDest = "HandleMarkerClear",
    TogglePDA = "_HandleToggleEvent"
  },
  Children = {
    {
      name = "PDA Subtitle Buffer",
      x1 = 164,
      y1 = 368,
      x2 = 464,
      y2 = 438,
      RedLevel = 16,
      GreenLevel = 32,
      BlueLevel = 32,
      TranslucencyLevel = 192,
      HorizontalAnchor = "center",
      VerticalAnchor = "center",
      visible = 1,
      container = false,
      WidgetType = "image",
      texture = nil,
      textureFile = nil,
      rotation = 0,
      u1 = 0,
      v1 = 0,
      u2 = 1,
      v2 = 1,
      EventHandlerFile = "MrxGuiTextBuffer",
      EventHandlers = {
        GuiInitialization = MrxGuiTextBuffer.HandleInstantiationEventForTextBuffer
      },
      EventHandlerNames = {
        GuiInitialization = "HandleInstantiationEventForTextBuffer"
      },
      Children = {}
    }
  }
}
if import then
  import("MrxGuiBase")
end

function ReInit()
  for i in pairs(AddedWidgetList) do
    MrxGuiBase.RemoveWidget(AddedWidgetList[i])
  end
  AddedWidgetList = {}
  for i in pairs(LocalWidgetList) do
    MrxGuiBase.LoadAndAddWidgetFromLayoutFileData(LocalWidgetList[i], AddedWidgetList)
  end
end
