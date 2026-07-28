LocalWidgetList = {}
if import then
  import("MrxGuiLoadScreen")
else
  MrxGuiLoadScreen = {}
end
LocalWidgetList[1] = {
  name = "Loading Screen",
  x1 = 0,
  y1 = 0,
  x2 = 640,
  y2 = 480,
  RedLevel = 255,
  GreenLevel = 255,
  BlueLevel = 255,
  TranslucencyLevel = 0,
  HorizontalAnchor = "left",
  VerticalAnchor = "top",
  visible = 1,
  container = true,
  EventHandlerFile = "MrxGuiLoadScreen",
  EventHandlers = {
    LoadStateChange = MrxGuiLoadScreen.HandleStateChangeEvent,
    GuiInitialization = MrxGuiLoadScreen.HandleInit
  },
  EventHandlerNames = {
    LoadStateChange = "HandleStateChangeEvent",
    GuiInitialization = "HandleInit"
  },
  Children = {
    {
      name = "Loading background",
      x1 = 0,
      y1 = 0,
      x2 = 640,
      y2 = 480,
      RedLevel = 0,
      GreenLevel = 0,
      BlueLevel = 0,
      TranslucencyLevel = 255,
      HorizontalAnchor = "left",
      VerticalAnchor = "top",
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
      EventHandlerFile = "",
      EventHandlers = {},
      EventHandlerNames = {},
      Children = {}
    },
    {
      name = "Loading text",
      x1 = 56,
      y1 = 416,
      x2 = 111,
      y2 = 432,
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255,
      TranslucencyLevel = 255,
      HorizontalAnchor = "left",
      VerticalAnchor = "bottom",
      visible = 1,
      container = false,
      WidgetType = "text",
      text = "Loading",
      font = "font_16",
      scale = 1,
      Justification = "left",
      EventHandlerFile = "",
      EventHandlers = {},
      EventHandlerNames = {},
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
