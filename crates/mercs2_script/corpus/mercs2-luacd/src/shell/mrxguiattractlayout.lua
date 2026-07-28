LocalWidgetList = {}
if import then
  import("MrxGuiAttractMode")
else
  MrxGuiAttractMode = {}
end
LocalWidgetList[1] = {
  name = "Attract",
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
  container = true,
  EventHandlerFile = "MrxGuiAttractMode",
  EventHandlers = {
    GuiInitialization = MrxGuiAttractMode.HandleInit
  },
  EventHandlerNames = {GuiInitialization = "HandleInit"},
  Children = {
    {
      name = "attract bg",
      x1 = 0,
      y1 = 0,
      x2 = 640,
      y2 = 480,
      RedLevel = 0,
      GreenLevel = 0,
      BlueLevel = 0,
      TranslucencyLevel = 255,
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
