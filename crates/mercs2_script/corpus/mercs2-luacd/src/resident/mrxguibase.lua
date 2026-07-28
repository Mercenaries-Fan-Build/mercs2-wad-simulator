Joystick = {
  BUTTON_PAD1_U = 1,
  BUTTON_PAD1_D = 2,
  BUTTON_PAD1_L = 3,
  BUTTON_PAD1_R = 4,
  BUTTON_PAD2_U = 5,
  BUTTON_PAD2_D = 6,
  BUTTON_PAD2_L = 7,
  BUTTON_PAD2_R = 8,
  BUTTON_L_STICK_L = 9,
  BUTTON_L_STICK_R = 10,
  BUTTON_L_STICK_U = 11,
  BUTTON_L_STICK_D = 12,
  BUTTON_R_STICK_L = 13,
  BUTTON_R_STICK_R = 14,
  BUTTON_R_STICK_U = 15,
  BUTTON_R_STICK_D = 16,
  BUTTON_ALT1_1 = 17,
  BUTTON_ALT1_2 = 18,
  BUTTON_ALT1_3 = 19,
  BUTTON_ALT2_1 = 20,
  BUTTON_ALT2_2 = 21,
  BUTTON_ALT2_3 = 22,
  BUTTON_SYS1 = 23,
  BUTTON_SYS2 = 24,
  BUTTON_L_STICK_LR = 25,
  BUTTON_USE_MELEE = 26,
  BUTTON_USE_RELOAD = 27
}
ControlFocusQueue = {}
ControlModeManager = {}

function GetControlFocus(oWidget, bPause, bGlobal)
  if nil == bPause then
    bPause = false
  end
  local uOwner = oWidget:GetOwner()
  if nil == uOwner or bGlobal then
    uOwner = "global"
  end
  if not ControlFocusQueue[uOwner] then
    ControlFocusQueue[uOwner] = {}
  end
  table.insert(ControlFocusQueue[uOwner], 1, {oWidget = oWidget, bPause = bPause})
  if nil ~= ControlModeManager[uOwner] then
    if ControlModeManager[uOwner] then
      SetDialogBoxMode(uOwner, false)
    else
      SetSupportMenuMode(uOwner, false)
    end
  end
  ControlModeManager[uOwner] = bPause
  if bPause then
    SetDialogBoxMode(uOwner, true)
  else
    SetSupportMenuMode(uOwner, true)
  end
end

function ReleaseControlFocus(oWidget, uUseOwnerGuid, bWasGlobal)
  local uOwner
  if uUseOwnerGuid then
    uOwner = uUseOwnerGuid
  else
    uOwner = oWidget:GetOwner()
  end
  if nil == uOwner or bWasGlobal then
    uOwner = "global"
  end
  local tQueue = ControlFocusQueue[uOwner]
  if not tQueue then
    return
  end
  local nFoundIndex
  for nIndex, tData in pairs(ControlFocusQueue[uOwner]) do
    if oWidget == tData.oWidget then
      nFoundIndex = nIndex
    end
  end
  if not nFoundIndex then
    return
  end
  table.remove(ControlFocusQueue[uOwner], nFoundIndex)
  if nil ~= ControlModeManager[uOwner] then
    if ControlModeManager[uOwner] then
      SetDialogBoxMode(uOwner, false)
    else
      SetSupportMenuMode(uOwner, false)
    end
  end
  local tNextData = ControlFocusQueue[uOwner][1]
  if tNextData then
    ControlModeManager[uOwner] = tNextData.bPause
    if tNextData.bPause then
      SetDialogBoxMode(uOwner, true)
    else
      SetSupportMenuMode(uOwner, true)
    end
  end
end

function GetCurrentControlHolder(uOwnerGuid)
  local uOwner = uOwnerGuid
  if nil == uOwner then
    uOwner = "global"
  end
  if ControlFocusQueue[uOwner] and ControlFocusQueue[uOwner][1] then
    return ControlFocusQueue[uOwner][1].oWidget
  end
  return nil
end

function InformControlOwnerChanged(oWidget, uPreviousOwner)
end

function IsPlayerLocal(uPlayerGuid)
  if nil == uPlayerGuid then
    return true
  end
  if not Net.IsMultiplayer() then
    return true
  end
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  local uViewportId = Player.GetViewportId(uPlayerGuid)
  return IsViewportLocal(uViewportId)
end

function IsViewportLocal(uViewportId)
  if nil == uViewportId then
    return true
  end
  if not Net.IsMultiplayer() then
    return true
  end
  if "userdata" ~= type(uViewportId) then
    return false
  end
  local nViewportId = tonumber(Sys.GuidToString(uViewportId))
  if 1 < nViewportId then
    return false
  end
  return true
end

EventManager = {
  EventList = {},
  RenderEvent = {EventType = 2},
  ScriptEventList = {},
  SendEvent = function(tEvent)
    if "table" == type(tEvent) and tEvent.EventType then
      table.insert(EventManager.ScriptEventList, tEvent)
    end
  end,
  ProcessEvents = function()
    table.insert(EventManager.EventList, RenderEvent)
    for nIndex, tEvent in pairs(EventManager.ScriptEventList) do
      table.insert(EventManager.EventList, tEvent)
      EventManager.ScriptEventList[nIndex] = nil
    end
    for nIndex, Event in pairs(EventManager.EventList) do
      if 8 == Event.EventType then
        for uId in pairs(WidgetIdIndex) do
          _GuiInternal.CorrectWidgetForResolution(uId)
        end
      elseif "ControllerInput" == Event.EventType then
        local oHoldingWidget = GetCurrentControlHolder(Event.uPlayerGuid)
        if oHoldingWidget and oHoldingWidget.BasicData.bEnabled and oHoldingWidget.EventHandlers[Event.EventType] then
          oHoldingWidget.EventHandlers[Event.EventType](oHoldingWidget, Event)
        end
      elseif WidgetManager.WidgetEventIndex[Event.EventType] ~= nil then
        for nWidgetIndex, Widget in pairs(WidgetManager.WidgetEventIndex[Event.EventType]) do
          if Widget.EventHandlers[Event.EventType] ~= nil and Widget.BasicData.bEnabled and (Event.uPlayerGuid == Widget.BasicData.uOwnerGuid or nil == Event.uPlayerGuid) then
            Widget.EventHandlers[Event.EventType](Widget, Event)
          end
        end
      end
      EventManager.EventList[nIndex] = nil
    end
  end
}

function SentEvent(tEvent)
  EventManager.SendEvent(tEvent)
end

function ProcessEventImmediate(tEvent)
  local vEventType = tEvent.EventType
  if 8 == vEventType then
    for uId in pairs(WidgetIdIndex) do
      _GuiInternal.CorrectWidgetForResolution(uId)
    end
  elseif "ControllerInput" == vEventType then
    local oHoldingWidget = GetCurrentControlHolder(tEvent.uPlayerGuid)
    if oHoldingWidget and oHoldingWidget.BasicData.bEnabled and oHoldingWidget.EventHandlers[vEventType] then
      oHoldingWidget.EventHandlers[vEventType](oHoldingWidget, tEvent)
    end
  elseif WidgetManager.WidgetEventIndex[vEventType] ~= nil then
    for nWidgetIndex, Widget in pairs(WidgetManager.WidgetEventIndex[vEventType]) do
      if Widget.EventHandlers[vEventType] ~= nil and Widget.BasicData.bEnabled and (tEvent.uPlayerGuid == Widget.BasicData.uOwnerGuid or nil == tEvent.uPlayerGuid) then
        Widget.EventHandlers[vEventType](Widget, tEvent)
      end
    end
  end
end

WidgetManager = {
  WidgetList = {},
  WidgetEventIndex = {},
  WidgetNameIndex = {},
  WidgetNamePlayerIndex = {},
  AddWidget = function(widget)
    for nIndex, oCheckWidget in pairs(WidgetManager.WidgetList) do
      if oCheckWidget == widget then
        return
      end
    end
    table.insert(WidgetManager.WidgetList, widget)
    for nEventIndex in pairs(widget.EventHandlers) do
      if WidgetManager.WidgetEventIndex[nEventIndex] == nil then
        WidgetManager.WidgetEventIndex[nEventIndex] = {}
      end
      table.insert(WidgetManager.WidgetEventIndex[nEventIndex], widget)
    end
    if widget.BasicData.name then
      if not WidgetManager.WidgetNameIndex[widget.BasicData.name] then
        WidgetManager.WidgetNameIndex[widget.BasicData.name] = {}
      end
      table.insert(WidgetManager.WidgetNameIndex[widget.BasicData.name], widget)
    end
    if widget.BasicData.name and widget.BasicData.uOwnerGuid then
      if not WidgetManager.WidgetNamePlayerIndex[widget.BasicData.uOwnerGuid] then
        WidgetManager.WidgetNamePlayerIndex[widget.BasicData.uOwnerGuid] = {}
      end
      WidgetManager.WidgetNamePlayerIndex[widget.BasicData.uOwnerGuid][widget.BasicData.name] = widget
    end
    widget:SetEnabled(true)
    _GuiInternal.ActivateWidget(widget.BasicData.uId, true)
  end,
  RemoveWidget = function(widget)
    for nWidgetIndex, nCheckWidget in pairs(WidgetManager.WidgetList) do
      if nCheckWidget == widget then
        table.remove(WidgetManager.WidgetList, nWidgetIndex)
      end
    end
    for nEventType in pairs(widget.EventHandlers) do
      if WidgetManager.WidgetEventIndex[nEventType] ~= nil then
        for nEventListIndex in pairs(WidgetManager.WidgetEventIndex[nEventType]) do
          if WidgetManager.WidgetEventIndex[nEventType][nEventListIndex] == widget then
            table.remove(WidgetManager.WidgetEventIndex[nEventType], nEventListIndex)
          end
        end
      end
    end
    if widget.BasicData.name and WidgetManager.WidgetNameIndex[widget.BasicData.name] then
      local nFoundIndex
      for nIndex, oWidget in pairs(WidgetManager.WidgetNameIndex[widget.BasicData.name]) do
        if oWidget == widget then
          nFoundIndex = nIndex
        end
      end
      if nFoundIndex then
        table.remove(WidgetManager.WidgetNameIndex[widget.BasicData.name], nFoundIndex)
      end
    end
    if widget.BasicData.name and widget.BasicData.uOwnerGuid and WidgetManager.WidgetNamePlayerIndex[widget.BasicData.uOwnerGuid] then
      WidgetManager.WidgetNamePlayerIndex[widget.BasicData.uOwnerGuid][widget.BasicData.name] = nil
    end
    widget:SetEnabled(false)
    _GuiInternal.ActivateWidget(widget.BasicData.uId, false)
  end,
  RemoveAll = function()
    WidgetManager.WidgetList = {}
    WidgetManager.WidgetEventIndex = {}
    WidgetManager.WidgetNameIndex = {}
    WidgetManager.WidgetNamePlayerIndex = {}
  end,
  UpdateWidgetEventHandlers = function(widget)
    WidgetManager.RemoveWidget(widget)
    WidgetManager.AddWidget(widget)
  end,
  GetWidgetByName = function(sWidgetName)
    if sWidgetName and WidgetManager.WidgetNameIndex[sWidgetName] then
      return WidgetManager.WidgetNameIndex[sWidgetName][#WidgetManager.WidgetNameIndex[sWidgetName]]
    else
      return nil
    end
  end,
  GetAllWidgetsByName = function(sWidgetName)
    if sWidgetName and WidgetManager.WidgetNameIndex[sWidgetName] then
      return WidgetManager.WidgetNameIndex[sWidgetName]
    else
      return {}
    end
  end,
  GetWidgetByIndexNumber = function(nIndex)
    if nIndex then
      return WidgetManager.WidgetList[nIndex]
    else
      return nil
    end
  end,
  GetWidgetByNameAndOwner = function(sWidgetName, uOwnerGuid)
    if "string" == type(sWidgetName) and "userdata" == type(uOwnerGuid) and WidgetManager.WidgetNamePlayerIndex[uOwnerGuid] and WidgetManager.WidgetNamePlayerIndex[uOwnerGuid][sWidgetName] then
      return WidgetManager.WidgetNamePlayerIndex[uOwnerGuid][sWidgetName]
    end
    return nil
  end
}
WidgetIdIndex = {}

function PushAllTextToFront()
  for nIndex, oWidget in pairs(WidgetManager.WidgetList) do
    if "text" == oWidget.BasicData.type then
      PushWidgetToFront(oWidget)
    end
  end
end

function AddWidget(WidgetToAdd)
  WidgetManager.AddWidget(WidgetToAdd)
end

function AddWidgetWithChildren(WidgetToAdd)
  WidgetManager.AddWidget(WidgetToAdd)
  local tChildren = WidgetToAdd:GetChildren()
  for Index, oChild in pairs(tChildren) do
    if "table" == type(oChild) then
      AddWidgetWithChildren(oChild)
    end
  end
end

function RemoveWidget(WidgetToRemove)
  WidgetManager.RemoveWidget(WidgetToRemove)
end

function RemoveWidgetWithChildren(WidgetToRemove)
  WidgetManager.RemoveWidget(WidgetToRemove)
  local tChildren = WidgetToRemove:GetChildren()
  for Index, oChild in pairs(tChildren) do
    if "table" == type(oChild) then
      RemoveWidgetWithChildren(oChild)
    end
  end
end

function PushWidgetToFront(oWidget)
  if _GuiInternal.PushWidgetToFront then
    _GuiInternal.PushWidgetToFront(oWidget.BasicData.uId)
  end
end

function PushWidgetToBack(oWidget)
  if _GuiInternal.PushWidgetToBack then
    _GuiInternal.PushWidgetToBack(oWidget.BasicData.uId)
  end
end

function GetWidgetByName(sWidgetName)
  return WidgetManager.GetWidgetByName(sWidgetName)
end

function GetAllWidgetsByName(sWidgetName)
  return WidgetManager.GetAllWidgetsByName(sWidgetName)
end

function GetWidgetByNameAndOwner(sWidgetName, uOwnerGuid)
  return WidgetManager.GetWidgetByNameAndOwner(sWidgetName, uOwnerGuid)
end

function _DestroyWidget(oWidget)
  oWidget:delete()
end

function DeleteTransientWidgets(uPlayer)
  local tTransientWidgets = {}
  for uId, oWidget in pairs(WidgetIdIndex) do
    if oWidget and oWidget.BasicData.bTransient and oWidget:GetOwner() == uPlayer then
      table.insert(tTransientWidgets, oWidget)
    end
  end
  for n, oWidget in ipairs(tTransientWidgets) do
    RemoveWidget(oWidget)
    oWidget:delete()
  end
end

Widget = {
  BasicData = {bEnabled = false},
  CustomData = {},
  EventHandlers = {},
  __gc = _DestroyWidget
}

function Widget:new(NewWidget)
  NewWidget = NewWidget or {}
  NewWidget.BasicData = {
    uId = _GuiInternal.CreateWidget(),
    bTransient = true
  }
  NewWidget.CustomData = {}
  NewWidget.EventHandlers = {}
  setmetatable(NewWidget, self)
  self.__index = self
  WidgetIdIndex[NewWidget.BasicData.uId] = NewWidget
  return NewWidget
end

function Widget:New(NewWidget)
  return self:new(NewWidget)
end

function Widget:delete()
  WidgetIdIndex[self.BasicData.uId] = nil
  self.EventCallbackData = nil
  self.EventParamData = nil
  if "flash" == self.BasicData.type then
    Event.Create(Event.TimerRelative, {1}, _GuiInternal.DeleteWidget, {
      self.BasicData.uId
    })
  else
    _GuiInternal.DeleteWidget(self.BasicData.uId)
  end
end

function Widget:Delete()
  return self:delete()
end

function Widget:DeleteWithChildren()
  local tChildren = self:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:DeleteWithChildren()
  end
  self:delete()
end

function Widget:SetName(Name)
  self.BasicData.name = Name or self.BasicData.name
end

function Widget:GetName()
  return self.BasicData.name
end

function Widget:SetVisible(isVisible)
  _GuiInternal.SetWidgetVisible(self.BasicData.uId, isVisible)
  if not _GuiInternal.nVersion then
    local tChildren = self:GetChildren()
    for nIndex, oChild in pairs(tChildren) do
      if oChild and oChild.SetVisible then
        oChild:SetVisible(isVisible)
      end
    end
  end
end

function Widget:GetVisible()
  return _GuiInternal.GetWidgetVisible(self.BasicData.uId)
end

function Widget:SetSleeping(bSleeping)
  if _GuiInternal.SetWidgetSleep then
    _GuiInternal.SetWidgetSleep(self.BasicData.uId, bSleeping)
  end
end

function Widget:GetSleeping()
  return _GuiInternal.GetWidgetSleep(self.BasicData.uId)
end

function Widget:SetUseImmortalEvents(bUse)
  if bUse ~= self.BasicData.bImmortalEvents then
    self.BasicData.bImmortalEvents = bUse
    if self.BasicData.bEnabled then
      self:SetEnabled(false)
      self:SetEnabled(true)
    end
    local tChildren = self:GetChildren()
    for n, oChild in pairs(tChildren) do
      oChild:SetUseImmortalEvents(bUse)
    end
  end
end

function Widget:SetTransient(bTransient)
  if not bTransient then
    self.BasicData.bTransient = nil
  else
    self.BasicData.bTransient = bTransient
  end
end

_tOwnerRequiredEvents = {
  GuiAmmoUpdate = true,
  GuiMinimapUpdate = true,
  GuiHealthUpdate = true,
  GuiVehicleHealthUpdate = true,
  GuiReticleUpdate = true,
  GuiWeaponEquippedUpdate = true,
  GuiSupportMenuEnter = true,
  GuiPlayerReceiveDamage = true,
  GuiVehicleNameUpdate = true,
  GuiVehicleDisguiseUpdate = true
}

function Widget:SetEventHandler(EventType, EventHandlerFunction)
  if not EventType then
    return
  end
  if type(EventType) == "string" and "GuiInitialization" == EventType then
    self.EventHandlers.GuiInitialization = EventHandlerFunction
  elseif type(EventType) == "string" and "GuiUpdate" == EventType then
    self.EventHandlers[EventType] = EventHandlerFunction
    _GuiInternal.SetWidgetUpdateCallback(self.BasicData.uId, EventHandlerFunction, {self})
  elseif type(EventType) == "string" and "nil" ~= type(Event[EventType]) then
    self.EventHandlers[EventType] = EventHandlerFunction
    if self.EventHandlerData and nil ~= self.EventHandlerData[EventType] then
      Event.Delete(self.EventHandlerData[EventType])
      self.EventHandlerData[EventType] = nil
    end
    if not EventHandlerFunction then
      return
    end
    if _tOwnerRequiredEvents[EventType] and not self:GetOwner() then
      return
    end
    if self.BasicData.bEnabled then
      self.EventHandlerData = self.EventHandlerData or {}
      self.EventCallbackData = self.EventCallbackData or {self}
      self.EventParamData = self.EventParamData or {
        self.BasicData.uOwnerGuid
      }
      self.EventHandlerData[EventType] = Event.CreatePersistent(Event[EventType], self.EventParamData, EventHandlerFunction, self.EventCallbackData, self.BasicData.bImmortalEvents)
    end
  else
    local EventTypeNumber = EventType
    if EventTypeNumber then
      if EventHandlerFunction and not self.EventHandlers[EventTypeNumber] then
        if WidgetManager.WidgetEventIndex[EventTypeNumber] == nil then
          WidgetManager.WidgetEventIndex[EventTypeNumber] = {}
        end
        table.insert(WidgetManager.WidgetEventIndex[EventTypeNumber], self)
      end
      if not EventHandlerFunction and EventTypeNumber and WidgetManager.WidgetEventIndex[EventTypeNumber] then
        for i, oCheckWidget in pairs(WidgetManager.WidgetEventIndex[EventTypeNumber]) do
          if oCheckWidget == self then
            table.remove(WidgetManager.WidgetEventIndex[EventTypeNumber], i)
          end
        end
      end
      self.EventHandlers[EventTypeNumber] = EventHandlerFunction
    end
  end
end

function Widget:_InitAnimationData()
  self.AnimationPoints = {}
  self.AnimationData = {
    tPointQueue = {},
    fCompletion = nil,
    tCompletionData = {},
    bAnimating = false
  }
end

function Widget:AddAnimationPoint(tPoint)
  if not self.AnimationData then
    self:_InitAnimationData()
  end
  local nNewIndex = table.getn(self.AnimationPoints) + 1
  local tPointData = {}
  tPointData.RedLevel = tPoint.RedLevel
  tPointData.GreenLevel = tPoint.GreenLevel
  tPointData.BlueLevel = tPoint.BlueLevel
  tPointData.TranslucencyLevel = tPoint.TranslucencyLevel
  tPointData.nAnimationTime = tPoint.nAnimationTime or 0
  tPointData.nU1 = tPoint.nU1
  tPointData.nV1 = tPoint.nV1
  tPointData.nU2 = tPoint.nU2
  tPointData.nV2 = tPoint.nV2
  tPointData.nRotation = tPoint.nRotation
  tPointData.nRotationDirection = tPoint.nRotationDirection
  tPointData.nX1 = tPoint.x or tPoint.x1
  tPointData.nY1 = tPoint.y or tPoint.y1
  tPointData.nX2 = tPoint.x2 or tPoint.x1
  tPointData.nY2 = tPoint.y2 or tPoint.y1
  if not tPoint.x and tPoint.x1 and not tPoint.x2 then
    tPointData.nX2 = nil
  end
  if not tPoint.y and tPoint.y1 and not tPoint.y2 then
    tPointData.nY2 = nil
  end
  table.insert(self.AnimationPoints, tPointData)
  return nNewIndex
end

function Widget:SetAnimationPoint(nPointNumber, tPoint)
  if "number" ~= type(nPointNumber) then
    return
  end
  if "table" ~= type(tPoint) then
    return
  end
  if not self.AnimationData then
    self:_InitAnimationData()
  end
  local tPointData = {}
  tPointData.RedLevel = tPoint.RedLevel
  tPointData.GreenLevel = tPoint.GreenLevel
  tPointData.BlueLevel = tPoint.BlueLevel
  tPointData.TranslucencyLevel = tPoint.TranslucencyLevel
  tPointData.nAnimationTime = tPoint.nAnimationTime or 0
  tPointData.nU1 = tPoint.nU1
  tPointData.nV1 = tPoint.nV1
  tPointData.nU2 = tPoint.nU2
  tPointData.nV2 = tPoint.nV2
  tPointData.nRotation = tPoint.nRotation
  tPointData.nRotationDirection = tPoint.nRotationDirection
  tPointData.nX1 = tPoint.x or tPoint.x1
  tPointData.nY1 = tPoint.y or tPoint.y1
  tPointData.nX2 = tPoint.x2 or tPoint.x1
  tPointData.nY2 = tPoint.y2 or tPoint.y1
  if not tPoint.x and tPoint.x1 and not tPoint.x2 then
    tPointData.nX2 = nil
  end
  if not tPoint.y and tPoint.y1 and not tPoint.y2 then
    tPointData.nY2 = nil
  end
  self.AnimationPoints[nPointNumber] = tPointData
end

function Widget:AnimateToPoint(nPointNumber, nTime, bImmediate, fComplete, tUserData, nElapsedTime)
  if "number" ~= type(nPointNumber) then
    return
  end
  if "table" ~= type(self.AnimationPoints[nPointNumber]) then
    return
  end
  if not self.AnimationData then
    return
  end
  local tPoint = self.AnimationPoints[nPointNumber]
  nTime = ValidateParameter(nTime, "number", tPoint.nAnimationTime)
  nTime = ValidateParameter(nTime, "number", 0)
  fComplete = ValidateParameter(fComplete, "function", nil)
  tUserData = ValidateParameter(tUserData, "table", nil)
  nElapsedTime = ValidateParameter(nElapsedTime, "number", nil)
  tPoint.nAnimationTime = nTime
  bImmediate = ValidateParameter(bImmediate, "boolean", false)
  if bImmediate then
    self.AnimationData.tPointQueue = {}
    self.AnimationData.nTimeRemaining = -1
    self.AnimationData.fCompletion = nil
    self.AnimationData.bAnimating = false
  end
  local tDataStuff = {
    nPoint = nPointNumber,
    fCompletion = fComplete,
    tCompletionData = tUserData,
    nElapsedTime = nElapsedTime
  }
  table.insert(self.AnimationData.tPointQueue, tDataStuff)
  if not self.AnimationData.bAnimating then
    _HandleAnimationComplete(self)
  end
  self.AnimationData.bAnimating = true
end

function Widget:IsAnimating()
  return self.AnimationData and self.AnimationData.bAnimating
end

function _HandleAnimationComplete(self)
  ASSERT(self.AnimationData)
  if self.AnimationData.bHandlingAnimationComplete then
    return
  end
  self.AnimationData.bHandlingAnimationComplete = true
  if self.AnimationData.fCompletion then
    local tData = self.AnimationData.tCompletionData or {}
    table.insert(tData, 1, self)
    self.AnimationData.fCompletion(unpack(tData))
  end
  self.AnimationData.fCompletion = nil
  self.AnimationData.tCompletionData = nil
  local nNextPoint, nElapsedTime
  while not nNextPoint do
    if 1 > table.getn(self.AnimationData.tPointQueue) then
      self.AnimationData.bAnimating = false
      self.AnimationData.bHandlingAnimationComplete = nil
      return
    end
    if self.AnimationData.tPointQueue[1] then
      nNextPoint = self.AnimationData.tPointQueue[1].nPoint
    end
    if not self.AnimationPoints[nNextPoint] then
      nNextPoint = nil
    end
    self.AnimationData.fCompletion = self.AnimationData.tPointQueue[1].fCompletion
    self.AnimationData.tCompletionData = self.AnimationData.tPointQueue[1].tCompletionData
    nElapsedTime = self.AnimationData.tPointQueue[1].nElapsedTime
    table.remove(self.AnimationData.tPointQueue, 1)
  end
  local tNextPoint = self.AnimationPoints[nNextPoint]
  self.AnimationData.bAnimating = true
  local nSelfX1, nSelfY1, nSelfX2, nSelfY2 = self:GetLocation()
  local bMaintainDimensions = false
  if tNextPoint.nX1 and tNextPoint.nY1 and not tNextPoint.nX2 and not tNextPoint.nY2 then
    bMaintainDimensions = true
  end
  if bMaintainDimensions then
    _GuiInternal.InterpolateWidget(self.BasicData.uId, tNextPoint.nAnimationTime, tNextPoint.nX1 or nSelfX1, tNextPoint.nY1 or nSelfY1, nil, nil, tNextPoint.RedLevel or -4096, tNextPoint.GreenLevel or -4096, tNextPoint.BlueLevel or -4096, tNextPoint.TranslucencyLevel or -4096, _HandleAnimationComplete, {self}, tNextPoint.nU1, tNextPoint.nV1, tNextPoint.nU2, tNextPoint.nV2, tNextPoint.nRotation, tNextPoint.nRotationDirection, nElapsedTime)
  else
    _GuiInternal.InterpolateWidget(self.BasicData.uId, tNextPoint.nAnimationTime, tNextPoint.nX1 or nSelfX1, tNextPoint.nY1 or nSelfY1, tNextPoint.nX2 or nSelfX2, tNextPoint.nY2 or nSelfY2, tNextPoint.RedLevel or -4096, tNextPoint.GreenLevel or -4096, tNextPoint.BlueLevel or -4096, tNextPoint.TranslucencyLevel or -4096, _HandleAnimationComplete, {self}, tNextPoint.nU1, tNextPoint.nV1, tNextPoint.nU2, tNextPoint.nV2, tNextPoint.nRotation, tNextPoint.nRotationDirection, nElapsedTime)
  end
  self.AnimationData.bHandlingAnimationComplete = nil
end

function Widget:SetEnabled(bEnabled)
  if bEnabled == self.BasicData.bEnabled then
    return
  end
  if bEnabled then
    for oEventType in pairs(self.EventHandlers) do
      if Event[oEventType] and (not _tOwnerRequiredEvents[oEventType] or not not self:GetOwner()) then
        self.EventHandlerData = self.EventHandlerData or {}
        self.EventHandlerData[oEventType] = Event.CreatePersistent(Event[oEventType], {
          self.BasicData.uOwnerGuid
        }, self.EventHandlers[oEventType], {self}, self.BasicData.bImmortalEvents)
      end
    end
  elseif self.EventHandlerData then
    for oEventType in pairs(self.EventHandlerData) do
      if nil ~= self.EventHandlerData[oEventType] then
        Event.Delete(self.EventHandlerData[oEventType])
        self.EventHandlerData[oEventType] = nil
      end
    end
  end
  self.BasicData.bEnabled = bEnabled
end

function Widget:SetLocation(x, y, x1, y1)
  _GuiInternal.SetWidgetLocation(self.BasicData.uId, x, y, x1, y1, true)
end

function Widget:SetHighlightable(setting)
  _GuiInternal.SetWidgetHighlightable(self.BasicData.uId, setting)
end

function Widget:SetCoordinates(x, y, x1, y1)
  local nX1, nY1, nX2, nY2 = _GuiInternal.GetWidgetLocation(self.BasicData.uId)
  _GuiInternal.SetWidgetLocation(self.BasicData.uId, x or nX1, y or nY1, x1 or nX2, y1 or nY2, false)
end

function Widget:GetLocation()
  return _GuiInternal.GetWidgetLocation(self.BasicData.uId)
end

function Widget:SetCorrectedLocation(nX1, nY1, nX2, nY2)
  if _GuiInternal.SetWidgetCorrectedLocation then
    return _GuiInternal.SetWidgetCorrectedLocation(self.BasicData.uId, nX1, nY1, nX2, nY2)
  end
end

function Widget:GetCorrectedLocation()
  if _GuiInternal.GetWidgetCorrectedLocation then
    return _GuiInternal.GetWidgetCorrectedLocation(self.BasicData.uId)
  end
end

function Widget:SetColor(r, g, b, a, bSuppressPropogation)
  _GuiInternal.SetWidgetColor(self.BasicData.uId, r, g, b, a, not bSuppressPropogation)
end

function Widget:GetColor()
  return _GuiInternal.GetWidgetColor(self.BasicData.uId)
end

function Widget:SetTranslucency(level, bSuppressPropogation)
  _GuiInternal.SetWidgetColor(self.BasicData.uId, -255, -255, -255, level, not bSuppressPropogation)
end

function Widget:GetTranslucency()
  local nSelfR, nSelfG, nSelfB, nSelfA = _GuiInternal.GetWidgetColor(self.BasicData.uId)
  return nSelfA
end

function Widget:SetAnchoring(sHorizontalAnchor, sVerticalAnchor)
  local nHorizAnchor, nVertAnchor = _GuiInternal.GetWidgetAnchoring(self.BasicData.uId)
  if "left" == sHorizontalAnchor then
    nHorizAnchor = -1
  elseif "right" == sHorizontalAnchor then
    nHorizAnchor = 1
  elseif "center" == sHorizontalAnchor then
    nHorizAnchor = 0
  end
  if "top" == sVerticalAnchor then
    nVertAnchor = -1
  elseif "bottom" == sVerticalAnchor then
    nVertAnchor = 1
  elseif "center" == sVerticalAnchor then
    nVertAnchor = 0
  end
  _GuiInternal.SetWidgetAnchoring(self.BasicData.uId, nHorizAnchor, nVertAnchor)
end

function Widget:SetFullscreen(sType)
  _GuiInternal.SetWidgetFullscreen(self.BasicData.uId, sType)
end

function Widget:GetFullscreen()
  return nil
end

function Widget:Duplicate(oParent)
  local NewWidget = Widget:new()
  NewWidget:SetLocation(self:GetLocation())
  NewWidget:SetColor(self:GetColor())
  NewWidget:SetOwner(self:GetOwner())
  NewWidget:SetVisible(self:GetVisible())
  for nIndex in pairs(self.BasicData) do
    if nIndex ~= "uId" then
      NewWidget.BasicData[nIndex] = self.BasicData[nIndex]
    end
  end
  NewWidget.BasicData.bEnabled = false
  for nIndex in pairs(self.CustomData) do
    NewWidget.CustomData[nIndex] = self.CustomData[nIndex]
  end
  for Index in pairs(self.EventHandlers) do
    NewWidget:SetEventHandler(Index, self.EventHandlers[Index])
  end
  NewWidget.ParentWidget = oParent
  local tChildren = self:GetChildren()
  for Index, oChild in pairs(tChildren) do
    if oChild and oChild.Duplicate then
      NewWidget:AddChild(oChild:Duplicate(NewWidget))
    end
  end
  for Index in pairs(self) do
    if "table" ~= type(self[Index]) and "EventHandlerData" ~= Index then
      NewWidget[Index] = self[Index]
    end
  end
  return NewWidget
end

function Widget:SetOwner(uGuid)
  if nil ~= uGuid and "userdata" ~= type(uGuid) then
    return
  end
  if self.BasicData.uOwnerGuid == uGuid then
    return
  end
  local uOldGuid = self.BasicData.uOwnerGuid
  self.BasicData.uOwnerGuid = uGuid
  if self.BasicData.name then
    if uOldGuid and WidgetManager.WidgetNamePlayerIndex[uOldGuid] then
      WidgetManager.WidgetNamePlayerIndex[uOldGuid][self.BasicData.name] = nil
    end
    if self.BasicData.uOwnerGuid then
      if not WidgetManager.WidgetNamePlayerIndex[self.BasicData.uOwnerGuid] then
        WidgetManager.WidgetNamePlayerIndex[self.BasicData.uOwnerGuid] = {}
      end
      WidgetManager.WidgetNamePlayerIndex[self.BasicData.uOwnerGuid][self.BasicData.name] = self
    end
  end
  if self.BasicData.bEnabled then
    self:SetEnabled(false)
    self:SetEnabled(true)
  end
  _GuiInternal.SetWidgetViewport(self.BasicData.uId, Player.GetViewportId(uGuid))
end

function Widget:GetOwner()
  return self.BasicData.uOwnerGuid
end

function Widget:GetType()
  return self.BasicData.type
end

function Widget:GetChildren()
  local tIdList, nListSize = _GuiInternal.GetWidgetChildren(self.BasicData.uId)
  local tChildren = Table.Create(nListSize, 0)
  for nIndex, uId in pairs(tIdList) do
    Table.InsertI(tChildren, WidgetIdIndex[uId], nIndex)
  end
  return tChildren
end

function Widget:AddChild(oChild)
  _GuiInternal.AddWidgetChild(self.BasicData.uId, oChild.BasicData.uId)
end

function Widget:SetChild(nIndex, oChild)
  _GuiInternal.SetWidgetChild(self.BasicData.uId, oChild.BasicData.uId)
end

function Widget:RemoveChild(oChild)
  _GuiInternal.RemoveWidgetChild(self.BasicData.uId, oChild.BasicData.uId)
end

function Widget:RemoveAllChildren()
  _GuiInternal.RemoveAllWidgetChildren(self.BasicData.uId)
end

function Widget:SetIgnoresPause(bIgnore)
  _GuiInternal.SetWidgetIgnoresPause(self.BasicData.uId, bIgnore)
end

function Widget:GetIgnoresPause()
  return _GuiInternal.GetWidgetIgnoresPause(self.BasicData.uId)
end

TextWidget = {}

function TextWidget:new(NewWidget, uId)
  local NewWidget = NewWidget or {}
  NewWidget.BasicData = {
    type = "text",
    bEnabled = false,
    uId = uId or _GuiInternal.CreateTextWidget(),
    text = " ",
    font = "font_16",
    bTransient = true
  }
  NewWidget.CustomData = {}
  NewWidget.EventHandlers = {}
  setmetatable(NewWidget, self)
  self.__index = self
  WidgetIdIndex[NewWidget.BasicData.uId] = NewWidget
  return NewWidget
end

function TextWidget:SetText(t)
  if "" == t then
    t = " "
  end
  self.BasicData.text = t
  _GuiInternal.SetTextText(self.BasicData.uId, t)
end

function TextWidget:SetLocation(x, y, x1, y1)
  _GuiInternal.SetWidgetLocation(self.BasicData.uId, x, y, x1, y1)
end

function TextWidget:OffsetLocation(x, y)
  local nX1, nY1, nX2, nY2 = self:GetLocation()
  if self.CustomData.bWraps then
    self:SetLocation(nX1 + x, nY1 + y, nX2 + x, nY2 + y)
  else
    self:SetLocation(nX1 + x, nY1 + y)
  end
end

function TextWidget:SetFont(font)
  self.BasicData.font = font
  _GuiInternal.SetTextFont(self.BasicData.uId, font)
end

function TextWidget:GetFont(font)
  return self.BasicData.font
end

function TextWidget:SetJustification(sJustification)
  _GuiInternal.SetTextJustification(self.BasicData.uId, sJustification)
end

function TextWidget:GetJustification()
  return _GuiInternal.GetTextJustification(self.BasicData.uId)
end

function TextWidget:Wrap()
  self.CustomData.bWraps = true
  _GuiInternal.SetTextWrapping(self.BasicData.uId, true)
end

function TextWidget:SetWrapping(bWrap)
  self.CustomData.bWraps = bWrap
  _GuiInternal.SetTextWrapping(self.BasicData.uId, bWrap)
end

function TextWidget:GetText()
  return _GuiInternal.GetTextText(self.BasicData.uId)
end

function TextWidget:GetWidth()
  if _GuiInternal.GetTextWidth then
    return _GuiInternal.GetTextWidth(self.BasicData.uId)
  end
  local nX, nY, nX1 = self:GetLocation()
  return nX1 - nX
end

function TextWidget:GetHeight()
  return _GuiInternal.GetTextHeight(self.BasicData.uId)
end

function TextWidget:SetScale(nScale)
  return _GuiInternal.SetTextScale(self.BasicData.uId, nScale)
end

function TextWidget:GetScale()
  return _GuiInternal.GetTextScale(self.BasicData.uId)
end

function TextWidget:SplitIntoLines()
  if not _GuiInternal.SplitText then
    return {self}
  end
  local tIds = {
    _GuiInternal.SplitText(self.BasicData.uId)
  }
  local tLines = {}
  for nIndex, uLineId in pairs(tIds) do
    local oLine = TextWidget:new(nil, uLineId)
    oLine:SetFont(self:GetFont())
    if oLine:GetName() then
      oLine:SetName(self:GetName() .. " line " .. nIndex)
    end
    table.insert(tLines, oLine)
  end
  return tLines
end

function TextWidget:PerformTextAnimation(sType)
  if not _GuiInternal.AnimateText then
    return
  end
  _GuiInternal.AnimateText(self.BasicData.uId, sType)
end

function TextWidget:HaltTextAnimation()
  if not _GuiInternal.HaltTextAnimation then
    return
  end
  _GuiInternal.HaltTextAnimation(self.BasicData.uId)
end

function TextWidget:Duplicate(oParent)
  local NewWidget = TextWidget:new()
  NewWidget:SetLocation(self:GetLocation())
  NewWidget:SetColor(self:GetColor())
  NewWidget:SetOwner(self:GetOwner())
  NewWidget:SetText(self:GetText())
  NewWidget:SetFont(self:GetFont())
  NewWidget:SetScale(self:GetScale())
  for nIndex in pairs(self.BasicData) do
    if nIndex ~= "uId" then
      NewWidget.BasicData[nIndex] = self.BasicData[nIndex]
    end
  end
  NewWidget.BasicData.bEnabled = false
  for nIndex in pairs(self.CustomData) do
    NewWidget.CustomData[nIndex] = self.CustomData[nIndex]
  end
  for Index in pairs(self.EventHandlers) do
    NewWidget:SetEventHandler(Index, self.EventHandlers[Index])
  end
  NewWidget.ParentWidget = oParent
  local tChildren = self:GetChildren()
  for Index, oChild in pairs(tChildren) do
    if oChild and oChild.Duplicate then
      NewWidget:AddChild(oChild:Duplicate(NewWidget))
    end
  end
  for Index in pairs(self) do
    if "table" ~= type(self[Index]) then
      NewWidget[Index] = self[Index]
    end
  end
  return NewWidget
end

ImageWidget = {}

function ImageWidget:new(NewWidget)
  local NewWidget = NewWidget or {}
  NewWidget.BasicData = {
    type = "image",
    bEnabled = false,
    uId = _GuiInternal.CreateImageWidget(),
    bTransient = true
  }
  NewWidget.CustomData = {}
  NewWidget.EventHandlers = {}
  setmetatable(NewWidget, self)
  self.__index = self
  WidgetIdIndex[NewWidget.BasicData.uId] = NewWidget
  return NewWidget
end

function ImageWidget:OffsetLocation(x, y)
  local offsetX = x or 0
  local offsetY = y or 0
  local nX1, nY1, nX2, nY2 = self:GetLocation()
  self:SetLocation(nX1 + offsetX, nY1 + offsetX, nX2 + offsetX, nY2 + offsetY)
end

function ImageWidget:Move(x, y)
  self:SetLocation(x, y)
end

function ImageWidget:SetTextureCoordinates(nU1, nV1, nU2, nV2)
  local nSelfU1, nSelfV1, nSelfU2, nSelfV2 = _GuiInternal.GetImageTextureCoordinates(self.BasicData.uId)
  _GuiInternal.SetImageTextureCoordinates(self.BasicData.uId, nU1 or nSelfU1, nV1 or nSelfV1, nU2 or nSelfU2, nV2 or nSelfV2)
end

function ImageWidget:GetTextureCoordinates()
  return _GuiInternal.GetImageTextureCoordinates(self.BasicData.uId)
end

function ImageWidget:SetTileSize(nTileWidth, nTileHeight)
  if _GuiInternal.SetImageTiling then
    _GuiInternal.SetImageTiling(self.BasicData.uId, nTileWidth, nTileHeight)
  end
end

function ImageWidget:SetTexture(TextureName)
  self.BasicData.sTextureName = TextureName
  _GuiInternal.SetImageTexture(self.BasicData.uId, TextureName)
end

function ImageWidget:GetTexture()
  return self.BasicData.sTextureName
end

function ImageWidget:SetRotation(nDegrees, nAnchorX, nAnchorY)
  self.BasicData.nRotation = nDegrees
  _GuiInternal.SetImageRotation(self.BasicData.uId, nDegrees)
end

function ImageWidget:GetRotation()
  return _GuiInternal.GetImageRotation(self.BasicData.uId)
end

function ImageWidget:SetClockAnimation(nElapsedTime, nTotalTime, bFill, bClockwise)
  nElapsedTime = nElapsedTime or 0
  nTotalTime = nTotalTime or 10
  _GuiInternal.SetImageClockAnimation(self.BasicData.uId, nElapsedTime, nTotalTime, bFill, bClockwise)
end

function ImageWidget:SetClockAnimationCallback(fCallback, tData)
  _GuiInternal.SetImageClockCallback(self.BasicData.uId, fCallback, tData)
end

function ImageWidget:GetClockElapsedTime()
  return _GuiInternal.GetImageClockElapsed(self.BasicData.uId)
end

function ImageWidget:SetPieSliceRender(fStartAngle, fEndAngle)
  if _GuiInternal.SetImagePieSliceRender then
    _GuiInternal.SetImagePieSliceRender(self.BasicData.uId, fStartAngle, fEndAngle)
  end
end

function ImageWidget:DisablePieSliceRender()
  if _GuiInternal.DisableImagePieSliceRender then
    _GuiInternal.DisableImagePieSliceRender(self.BasicData.uId)
  end
end

function ImageWidget:Duplicate(oParent)
  local NewWidget = ImageWidget:new()
  NewWidget:SetLocation(self:GetLocation())
  NewWidget:SetColor(self:GetColor())
  NewWidget:SetOwner(self:GetOwner())
  NewWidget:SetTexture(self:GetTexture())
  NewWidget:SetTextureCoordinates(self:GetTextureCoordinates())
  for nIndex in pairs(self.BasicData) do
    if nIndex ~= "uId" then
      NewWidget.BasicData[nIndex] = self.BasicData[nIndex]
    end
  end
  NewWidget.BasicData.bEnabled = false
  for nIndex in pairs(self.CustomData) do
    NewWidget.CustomData[nIndex] = self.CustomData[nIndex]
  end
  for Index in pairs(self.EventHandlers) do
    NewWidget:SetEventHandler(Index, self.EventHandlers[Index])
  end
  NewWidget.ParentWidget = oParent
  local tChildren = self:GetChildren()
  for Index, oChild in pairs(tChildren) do
    if oChild and oChild.Duplicate then
      NewWidget:AddChild(oChild:Duplicate(NewWidget))
    end
  end
  for Index in pairs(self) do
    if "table" ~= type(self[Index]) then
      NewWidget[Index] = self[Index]
    end
  end
  return NewWidget
end

FlashWidget = {}

function FlashWidget:new(NewWidget)
  if _GuiInternal.CreateFlashWidget then
    local NewWidget = NewWidget or {}
    NewWidget.BasicData = {
      type = "flash",
      bEnabled = false,
      uId = _GuiInternal.CreateFlashWidget(),
      bTransient = true
    }
    NewWidget.CustomData = {}
    NewWidget.EventHandlers = {}
    setmetatable(NewWidget, self)
    self.__index = self
    WidgetIdIndex[NewWidget.BasicData.uId] = NewWidget
    NewWidget:SetEventHandler("ControllerInput", _HandleInputForFlashWidget)
    return NewWidget
  else
    return Widget:new(NewWidget)
  end
end

function FlashWidget:SetSwfFile(sSwfName, fCallback, tData)
  self.BasicData.sSwfName = sSwfName
  _GuiInternal.SetFlashSwfFile(self.BasicData.uId, sSwfName, fCallback, tData)
end

function FlashWidget:GetSwfFile()
  return self.BasicData.sSwfName
end

function FlashWidget:SetPlaySpeed(nSpeed)
  if _GuiInternal.SetFlashPlaySpeed then
    _GuiInternal.SetFlashPlaySpeed(self.BasicData.uId, nSpeed)
  end
end

function FlashWidget:GetPlaySpeed(nSpeed)
  if _GuiInternal.GetFlashPlaySpeed then
    return _GuiInternal.GetFlashPlaySpeed(self.BasicData.uId, nSpeed)
  end
end

function FlashWidget:Pause()
  if _GuiInternal.PauseFlash then
    _GuiInternal.PauseFlash(self.BasicData.uId)
  end
end

function FlashWidget:Play()
  if _GuiInternal.PlayFlash then
    _GuiInternal.PlayFlash(self.BasicData.uId)
  end
end

function FlashWidget:Restart()
  if _GuiInternal.RestartFlash then
    _GuiInternal.RestartFlash(self.BasicData.uId)
  end
end

function FlashWidget:SetFlashEventHandler(sEvent, fCallback, tCallbackData)
  if _GuiInternal.SetFlashCallback then
    _GuiInternal.SetFlashCallback(self.BasicData.uId, sEvent, _FlashCallback, {
      self,
      fCallback,
      tCallbackData
    })
  end
end

function FlashWidget:CallActionScriptCallback(sName, tArgs)
  table.insert(tArgs, 1, sName)
  table.insert(tArgs, 1, self.BasicData.uId)
  _GuiInternal.CallFlashScriptFunction(unpack(tArgs))
end

function FlashWidget:SetTesselationAllowed(bAllow)
  if _GuiInternal.SetFlashTesselationAllowed then
    _GuiInternal.SetFlashTesselationAllowed(self.BasicData.uId, bAllow)
  end
end

function _FlashCallback(oWidget, fFunction, tCallbackData, sParam)
  local tData = {}
  if "table" == type(tCallbackData) then
    for nIndex, vValue in ipairs(tCallbackData) do
      tData[nIndex] = vValue
    end
  end
  if "function" == type(fFunction) then
    table.insert(tData, 1, sParam)
    table.insert(tData, 1, oWidget)
    fFunction(unpack(tData))
  end
end

function _HandleInputForFlashWidget(oWidget, tEvent)
  for sKey, nValue in pairs(tEvent) do
    if string.find(sKey, "ButtonPress") then
      _GuiInternal.SendFlashInput(oWidget.BasicData.uId, nValue, "p")
    elseif string.find(sKey, "ButtonReleased") then
      _GuiInternal.SendFlashInput(oWidget.BasicData.uId, nValue, "r")
    end
  end
end

function FlashWidget:HandleLeftAnalogInput(nX, nY)
  if nX and nY and _GuiInternal.SendFlashLeftAnalogInput then
    _GuiInternal.SendFlashLeftAnalogInput(self.BasicData.uId, nX, nY)
  end
end

function FlashWidget:HandleRightAnalogInput(nX, nY)
  if nX and nY and _GuiInternal.SendFlashRightAnalogInput then
    _GuiInternal.SendFlashRightAnalogInput(self.BasicData.uId, nX, nY)
  end
end

SpriteWidget = {}

function SpriteWidget:new(NewWidget)
  local NewWidget = NewWidget or {}
  NewWidget.BasicData = {
    type = "sprite",
    bEnabled = false,
    uId = _GuiInternal.CreateSpriteWidget(),
    sTextureName = nil,
    bTransient = true
  }
  NewWidget.CustomData = {}
  NewWidget.EventHandlers = {}
  setmetatable(NewWidget, self)
  self.__index = self
  WidgetIdIndex[NewWidget.BasicData.uId] = NewWidget
  return NewWidget
end

function SpriteWidget:SetTexture(sTexture)
  _GuiInternal.SetSpriteTexture(self.BasicData.uId, sTexture)
end

function SpriteWidget:SetTextureSize(nWidth, nHeight)
  _GuiInternal.SetSpriteTextureSize(self.BasicData.uId, nWidth, nHeight)
end

function SpriteWidget:SetFrameSize(nWidth, nHeight)
  _GuiInternal.SetSpriteFrameSize(self.BasicData.uId, nWidth, nHeight)
end

function SpriteWidget:SetFrame(nFrame)
  _GuiInternal.SetSpriteFrame(self.BasicData.uId, nFrame)
end

function SpriteWidget:PlayAnimation(nStartFrame, nEndFrame, nTime, bLoop)
  _GuiInternal.AnimateSprite(self.BasicData.uId, nStartFrame, nEndFrame, nTime, bLoop)
end

function SpriteWidget:HaltAnimation()
  _GuiInternal.HaltSpriteAnimation(self.BasicData.uId)
end

MovieWidget = {}

function MovieWidget:new(NewWidget)
  if _GuiInternal.CreateMovieWidget then
    local NewWidget = NewWidget or {}
    NewWidget.BasicData = {
      type = "movie",
      bEnabled = false,
      uId = _GuiInternal.CreateMovieWidget(),
      bTransient = true
    }
    NewWidget.CustomData = {}
    NewWidget.EventHandlers = {}
    setmetatable(NewWidget, self)
    self.__index = self
    WidgetIdIndex[NewWidget.BasicData.uId] = NewWidget
    return NewWidget
  else
    return Widget:new(NewWidget)
  end
end

function MovieWidget:SetMovie(sFileName)
  _GuiInternal.SetMovieFile(self.BasicData.uId, sFileName)
end

function MovieWidget:Play(bLoop)
  _GuiInternal.PlayMovie(self.BasicData.uId, bLoop)
end

function MovieWidget:Stop()
  _GuiInternal.StopMovie(self.BasicData.uId)
end

function MovieWidget:Pause()
  _GuiInternal.PauseMovie(self.BasicData.uId)
end

function MovieWidget:SetEndCallback(fCallback, tData)
  _GuiInternal.SetMovieEndCallback(self.BasicData.uId, fCallback, tData)
end

function MovieWidget:GetCurrentFrame()
  if _GuiInternal.GetMovieCurrentFrameNumber then
    return _GuiInternal.GetMovieCurrentFrameNumber(self.BasicData.uId)
  end
  return -1
end

MinimapWidget = {}

function MinimapWidget:SetOwner(uGuid)
  Widget.SetOwner(self, uGuid)
  if _GuiInternal.SetMinimapOwner then
    _GuiInternal.SetMinimapOwner(self.BasicData.uId, uGuid)
  end
end

function MinimapWidget:SetUpMinimap(name, nXLoc, nYLoc, nRadius, texture, texWidth, texHeight, worldXMin, worldXMax, worldZMin, worldZMax, HorizAnchor, VertAnchor)
  self.BasicData.x = nXLoc
  self.BasicData.y = nYLoc
  self.BasicData.nRadius = nRadius
  self.BasicData.texture = texture
  self.BasicData.name = name
  self.BasicData.nTexWidth = texWidth or 512
  self.BasicData.nTexHeight = texHeight or -512
  self.BasicData.nWorldXMin = worldXMin or -512
  self.BasicData.nWorldXMax = worldXMax or 512
  self.BasicData.nWorldZMin = worldZMin or -512
  self.BasicData.nWorldZMax = worldZMax or 512
  self.BasicData.HorizontalAnchor = HorizAnchor or "left"
  self.BasicData.VerticalAnchor = VertAnchor or "top"
  self.BasicData.type = "minimap"
  self.NewObjectiveList = {}
  self.UpdatingObjectiveList = {}
  local uId = _GuiInternal.MinimapCreate(self.BasicData.x, self.BasicData.y, self.BasicData.nRadius, self.BasicData.texture, self.BasicData.nTexWidth, self.BasicData.nTexHeight, self.BasicData.HorizontalAnchor, self.BasicData.VerticalAnchor, self.BasicData.nWorldXMin, self.BasicData.nWorldXMax, self.BasicData.nWorldZMin, self.BasicData.nWorldZMax)
  if uId then
    self.BasicData.uId = uId
    WidgetIdIndex[uId] = self
  end
  self:SetEventHandler("GuiMinimapUpdate", MinimapDataUpdateHandler)
  WidgetManager.AddWidget(self)
end

function MinimapWidget:SetLocation(x, y)
end

function MinimapWidget:SetVisible(bVisible)
  _GuiInternal.SetWidgetVisible(self.BasicData.uId, bVisible)
  local tChildren = self:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    if oChild and oChild.SetVisible then
      oChild:SetVisible(isVisible)
    end
  end
end

function MinimapWidget:SetPlayerLocation(x, y, z)
  _GuiInternal.MinimapSetPlayerLocation(self.BasicData.uId, x, y, z)
end

function MinimapWidget:SetFocusLocation(x, y, z)
  _GuiInternal.MinimapSetFocusLocation(self.BasicData.uId, x, y, z)
end

function MinimapWidget:SetRotation(nRotation)
  _GuiInternal.MinimapSetRotation(self.BasicData.uId, nRotation)
end

function MinimapWidget:SetRange(nRange)
  _GuiInternal.MinimapSetRange(self.BasicData.uId, nRange)
end

function MinimapWidget:SetBorder(sBorderTexture, nTextureWidth, nTextureHeight)
  if _GuiInternal.SetMinimapBorder then
    _GuiInternal.SetMinimapBorder(self.BasicData.uId, sBorderTexture, nTextureWidth, nTextureHeight)
  end
end

function MinimapWidget:AddObjective(name, x, y, z, r, g, b, width, height, texture, uGuid, bSticky, bRotate, bOriented, nSortOrder)
  _GuiInternal.MinimapAddObjective(self.BasicData.uId, name, x or 0, y or 2, z or 0, r or 255, g or 255, b or 0, uGuid, width, height, texture, bSticky, bRotate, bOriented, nSortOrder)
end

function MinimapWidget:AnimateObjectiveSize(name, duration, minWidth, minHeight, maxWidth, maxHeight, bOneWay, speedWidth, speedHeight)
  if _GuiInternal.MinimapAnimateObjectiveSize then
    _GuiInternal.MinimapAnimateObjectiveSize(self.BasicData.uId, name, duration, minWidth, minHeight, maxWidth, maxHeight, bOneWay or false, speedWidth or 10, speedHeight or 10)
  end
end

function MinimapWidget:AnimateObjectiveAlpha(name, duration, minAlpha, maxAlpha, bOneWay, speed)
  if _GuiInternal.MinimapAnimateObjectiveAlpha then
    _GuiInternal.MinimapAnimateObjectiveAlpha(self.BasicData.uId, name, duration, minAlpha, maxAlpha, bOneWay or false, speed or 0.5)
  end
end

function MinimapWidget:AnimateObjectiveSonar(name, duration, sTexture, nTotalBlips, nVisibleBlips, nMinWidth, nMaxWidth, nBlipDelay, nAlphaAtMin, nAlphaAtMax, nGrowSpeed, r, g, b)
  if _GuiInternal.MinimapAnimateObjectiveSonar then
    _GuiInternal.MinimapAnimateObjectiveSonar(self.BasicData.uId, name, duration, sTexture, nTotalBlips, nVisibleBlips, nMinWidth, nMaxWidth, nBlipDelay, nAlphaAtMin, nAlphaAtMax, nGrowSpeed or 5, r or 255, g or 255, b or 255)
  end
end

function MinimapWidget:UnanimateObjective(name, type)
  if _GuiInternal.MinimapUnanimateObjective then
    _GuiInternal.MinimapUnanimateObjective(self.BasicData.uId, name, type or "all")
  end
end

function MinimapWidget:AddObjectiveWithGuid(sName, uGuid, nX, nY, nZ, nR, nG, nB, nWidth, nHeight, sTexture, bSticky, bRotate, bOriented, nSortOrder)
  if "userdata" ~= type(uGuid) then
    return
  end
  _GuiInternal.MinimapAddObjective(self.BasicData.uId, sName, nX or 0, nY or 2, nZ or 0, nR or 255, nG or 255, nB or 0, uGuid, nWidth, nHeight, sTexture, bSticky, bRotate, bOriented, nSortOrder)
end

function MinimapWidget:UpdateObjective(name, x, y, z, r, g, b, width, height, texture, bSticky, bRotate, bOriented, nSortOrder)
  self:AddObjective(name, x, y, z, r, g, b, width, height, texture, bSticky, bRotate, bOriented, nSortOrder)
end

function MinimapWidget:DeleteObjective(name)
  _GuiInternal.MinimapRemoveObjective(self.BasicData.uId, name)
end

function MinimapWidget:Delete()
  WidgetIdIndex[self.BasicData.uId] = nil
  _GuiInternal.MinimapDelete(self.BasicData.uId)
end

function MinimapDataUpdateHandler(Minimap, FocusX, FocusY, FocusZ, Rotation)
  _GuiInternal.MinimapUpdate(Minimap.BasicData.uId, Minimap.CustomData.nCorrectedX, Minimap.CustomData.nCorrectedY, FocusX, FocusY, FocusZ, FocusX, FocusY, FocusZ, Rotation)
  local uPlayer = Minimap:GetOwner()
  if uPlayer then
    local uCharacter = Player.GetControlledObject(uPlayer)
    if uCharacter then
      local nVelocity = Object.GetVelocity(uCharacter)
      if nVelocity then
        local nMinSpeed = 10
        local nMinRange = 150
        local nMaxSpeed = 50
        local nMaxRange = 400
        local nNewRange
        if nVelocity < nMinSpeed then
          nNewRange = nMinRange
        elseif nVelocity > nMaxSpeed then
          nNewRange = nMaxRange
        else
          nNewRange = nMinRange + (nVelocity - nMinSpeed) * (nMaxRange - nMinRange) / (nMaxSpeed - nMinSpeed)
        end
        _GuiInternal.MinimapSetRange(Minimap.BasicData.uId, nNewRange)
      end
    end
  end
end

function MinimapHandleE3HudModeEvent(oMinimap, tEvent)
  if tEvent.bOn then
    oMinimap:SetVisible(false)
  else
    oMinimap:SetVisible(true)
  end
end

function LoadGUIFile(sFile, fFinishedLoadingCallback, uOwnerGuid)
  local function fCallback(ModuleName)
    GUIFileLoadedCallback(ModuleName, uOwnerGuid)
    
    if "function" == type(fFinishedLoadingCallback) then
      fFinishedLoadingCallback(ModuleName)
    end
  end
  
  dynamic_import(sFile, fCallback)
end

function RemoveAllWidgetsInLayout(Module)
  if nil == Module then
    return
  end
  local i = 1
  local CurrentWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    CurrentWidget = Module.AddedWidgetList[i]
    RemoveWidgetWithChildren(CurrentWidget)
    i = i + 1
  end
end

function ReAddAllWidgets(Module)
  if nil == Module then
    return
  end
  local i = 1
  local CurrentWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    CurrentWidget = Module.AddedWidgetList[i]
    AddWidgetWithChildren(CurrentWidget)
    i = i + 1
  end
end

function HideAllWidgets(Module)
  if nil == Module then
    return
  end
  local i = 1
  local CurrentWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    CurrentWidget = Module.AddedWidgetList[i]
    CurrentWidget:SetVisible(false)
    i = i + 1
  end
end

function ShowAllWidgets(Module)
  if nil == Module then
    return
  end
  local i = 1
  local CurrentWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    CurrentWidget = Module.AddedWidgetList[i]
    CurrentWidget:SetVisible(true)
    i = i + 1
  end
end

function SetAllWidgetsSleep(Module, bSleep)
  if not Module then
    return
  end
  local i = 1
  local CurrentWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    CurrentWidget = Module.AddedWidgetList[i]
    CurrentWidget:SetSleeping(bSleep)
    i = i + 1
  end
end

function PushAllTextToFront(Module)
  if not Module then
    return
  end
  local i = 1
  local oWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    oWidget = Module.AddedWidgetList[i]
    if "text" == oWidget.BasicData.type then
      PushWidgetToFront(oWidget)
    end
    i = i + 1
  end
end

function AssignLayoutToPlayer(Module, uGuid)
  if nil == Module then
    return
  end
  if "userdata" ~= type(uGuid) then
    return
  end
  local i = 1
  local oCurrentWidget
  while Module.AddedWidgetList and Module.AddedWidgetList[i] do
    CurrentWidget = Module.AddedWidgetList[i]
    CurrentWidget:SetOwner(uGuid)
    i = i + 1
  end
end

function DuplicateLayout(Module)
  if "table" ~= type(Module) then
    return nil
  end
  if "table" ~= type(Module.AddedWidgetList) then
    return nil
  end
  local NewModule = {}
  NewModule.AddedWidgetList = {}
  NewModule.LocalWidgetList = Module.LocalWidgetList
  GUIFileLoadedCallback(NewModule)
  return NewModule
end

function GUIFileLoadedCallback(ModuleName, uOwnerGuid)
  ModuleName.AddedWidgetList = {}
  local i = 1
  while ModuleName.LocalWidgetList[i] do
    LoadAndAddWidgetFromLayoutFileData(ModuleName.LocalWidgetList[i], ModuleName.AddedWidgetList, nil, uOwnerGuid)
    i = i + 1
  end
  i = 1
  local CurrentWidget
  while ModuleName.AddedWidgetList and ModuleName.AddedWidgetList[i] do
    CurrentWidget = ModuleName.AddedWidgetList[i]
    if CurrentWidget.EventHandlers.GuiInitialization then
      CurrentWidget.EventHandlers.GuiInitialization(CurrentWidget, nil)
    end
    i = i + 1
  end
  for nIndex, oWidget in pairs(ModuleName.AddedWidgetList) do
    _GuiInternal.CorrectWidgetForResolution(oWidget.BasicData.uId)
  end
end

function LoadAndAddWidgetFromLayoutFileData(WidgetData, WidgetList, ParentWidget, uOwnerGuid)
  local NewWidget
  local WidgetPtr = Widget
  if WidgetData.container then
    NewWidget = Widget:new()
  elseif WidgetData.WidgetType == "image" then
    NewWidget = ImageWidget:new()
    NewWidget:SetTexture(WidgetData.texture)
    NewWidget:SetRotation(WidgetData.rotation)
    NewWidget:SetTextureCoordinates(WidgetData.u1 or 0, WidgetData.v1 or 0, WidgetData.u2 or 1, WidgetData.v2 or 1)
  elseif WidgetData.WidgetType == "text" then
    NewWidget = TextWidget:new()
    NewWidget:SetText(WidgetData.text)
    NewWidget:SetFont(WidgetData.font)
    NewWidget:SetScale(WidgetData.scale)
    NewWidget:SetJustification(WidgetData.Justification)
  elseif WidgetData.WidgetType == "minimap" then
    NewWidget = MinimapWidget:new()
    NewWidget:SetUpMinimap(WidgetData.name, WidgetData.x1 + WidgetData.nRadius, WidgetData.y1 + WidgetData.nRadius, WidgetData.nRadius, WidgetData.texture, WidgetData.nTextureWidth, WidgetData.nTextureHeight, WidgetData.nWorldXMin, WidgetData.nWorldXMax, WidgetData.nWorldZMin, WidgetData.nWorldZMax, WidgetData.HorizontalAnchor, WidgetData.VerticalAnchor)
    NewWidget:SetBorder(WidgetData.sBorderTexture, WidgetData.nBorderTextureWidth, WidgetData.nBorderTextureHeight)
  elseif WidgetData.WidgetType == "flash" then
    NewWidget = FlashWidget:new()
  elseif WidgetData.WidgetType == "sprite" then
    NewWidget = SpriteWidget:new()
    NewWidget:SetTexture(WidgetData.texture)
    NewWidget:SetRotation(WidgetData.rotation)
    NewWidget:SetTextureSize(WidgetData.textureWidth, WidgetData.textureHeight)
    NewWidget:SetFrameSize((WidgetData.u2 - WidgetData.u1) * WidgetData.textureWidth, (WidgetData.v2 - WidgetData.v1) * WidgetData.textureHeight)
    NewWidget:SetFrame(0)
  else
    NewWidget = Widget:new()
  end
  NewWidget:SetTransient(false)
  if WidgetData.WidgetType ~= "minimap" then
    NewWidget:SetLocation(WidgetData.x1, WidgetData.y1, WidgetData.x2, WidgetData.y2, true)
    NewWidget:SetName(WidgetData.name)
  end
  NewWidget:SetAnchoring(WidgetData.HorizontalAnchor, WidgetData.VerticalAnchor)
  if WidgetData.WidgetType == "image" and not WidgetData.container and WidgetData.nTileWidth and WidgetData.nTileHeight then
    NewWidget:SetTileSize(WidgetData.nTileWidth, WidgetData.nTileHeight)
  end
  if 0 < WidgetData.visible then
    NewWidget:SetVisible(true)
  else
    NewWidget:SetVisible(false)
  end
  if WidgetData.container then
    NewWidget:SetVisible(false)
  end
  NewWidget.BasicData.bContainer = WidgetData.container
  NewWidget:SetColor(WidgetData.RedLevel, WidgetData.GreenLevel, WidgetData.BlueLevel, WidgetData.TranslucencyLevel, true)
  NewWidget.ParentWidget = ParentWidget
  table.insert(WidgetList, NewWidget)
  if not WidgetData.WidgetType or WidgetData.WidgetType ~= "minimap" then
    WidgetManager.AddWidget(NewWidget)
  end
  for nEventIndex, fHandler in pairs(WidgetData.EventHandlers) do
    NewWidget:SetEventHandler(nEventIndex, fHandler)
  end
  if WidgetData.EventHandlers[0] then
    NewWidget:SetEventHandler(0, WidgetData.EventHandlers[0])
  end
  WidgetData.EventHandlerNames = nil
  WidgetData.EventHandlerFile = nil
  WidgetData.textureFile = nil
  if uOwnerGuid then
    NewWidget:SetOwner(uOwnerGuid)
  end
  local i = 1
  while WidgetData.Children[i] do
    local ChildWidget
    ChildWidget = LoadAndAddWidgetFromLayoutFileData(WidgetData.Children[i], WidgetList, NewWidget, uOwnerGuid)
    NewWidget:AddChild(ChildWidget)
    i = i + 1
  end
  return NewWidget
end

function UnloadGUIFile(vFile)
  dynamic_remove(vFile)
end

nWidgetSpaceScreenWidth = 640
nWidgetSpaceScreenHeight = 480
nScreenWidth = 640
nScreenHeight = 480
nScreenPositionX = 0
nScreenPositionY = 0
nPixelWidth = 1
nPixelHeight = 1
nScreenScaleFactor = 1
nScreenWidth = _G.g_nGuiScreenWidthTemp or 640
nScreenHeight = _G.g_nGuiScreenHeightTemp or 480
nScreenPositionX = _G.g_nGuiScreenPositionXTemp or 0
nScreenPositionY = _G.g_nGuiScreenPositionYTemp or 0
nPixelWidth = _G.g_nGuiScreenPixelWidthTemp or 1
nPixelHeight = _G.g_nGuiScreenPixelHeightTemp or 1
nScreenScaleFactor = nScreenHeight / nWidgetSpaceScreenHeight
_G.g_nGuiScreenWidthTemp = nil
_G.g_nGuiScreenHeightTemp = nil
_G.g_nGuiScreenPositionXTemp = nil
_G.g_nGuiScreenPositionYTemp = nil
_G.g_nGuiScreenPixelWidthTemp = nil
_G.g_nGuiScreenPixelHeightTemp = nil

local function abs(x)
  if x < 0 then
    return x * -1
  end
  return x
end

function ChangeScreenResolution(nWidth, nHeight, nNewPixelWidth, nNewPixelHeight, nX, nY)
  nScreenWidth = nWidth or nScreenWidth
  nScreenHeight = nHeight or nScreenHeight
  nPixelWidth = nNewPixelWidth or nPixelWidth
  nPixelHeight = nNewPixelHeight or nPixelHeight
  nScreenPositionX = nX or nScreenPositionX
  nScreenPositionY = nY or nScreenPositionY
  nScreenScaleFactor = nScreenHeight / nWidgetSpaceScreenHeight
  for iIndex, oWidget in pairs(WidgetManager.WidgetList) do
    if oWidget then
      oWidget.BasicData.bCorrectedForResolution = false
    end
  end
  for iIndex, oWidget in pairs(WidgetManager.WidgetList) do
    if not oWidget or not oWidget.BasicData.bCorrectedForResolution then
    end
  end
end

function ChangeScreenPixelSize(nWidth, nHeight)
  nPixelWidth = nWidth or nPixelWidth
  nPixelHeight = nHeight or nPixelHeight
  for iIndex, oWidget in pairs(WidgetManager.WidgetList) do
    if oWidget then
    end
  end
end

function ValidateParameter(Parameter, sType, DefaultValue)
  if type(Parameter) == sType then
    return Parameter
  else
    return DefaultValue
  end
end

function Run()
  EventManager.ProcessEvents()
end

function GetEventListTable()
  return EventManager.EventList
end

_SetDialogBoxMode = GuiSetDialogBoxMode
_G.GuiSetDialogBoxMode = nil
_SetSupportMenuMode = GuiSetSupportMenuMode
_G.GuiSetSupportMenuMode = nil

function SetDialogBoxMode(uPlayerGuid, bActivate)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if "boolean" ~= type(bActivate) then
    return false
  end
  if "function" ~= type(_SetDialogBoxMode) then
    return false
  end
  return _SetDialogBoxMode(uPlayerGuid, bActivate)
end

function SetSupportMenuMode(uPlayerGuid, bActivate)
  if "userdata" ~= type(uPlayerGuid) then
    return false
  end
  if "boolean" ~= type(bActivate) then
    return false
  end
  if "function" ~= type(_SetSupportMenuMode) then
    return false
  end
  return _SetSupportMenuMode(uPlayerGuid, bActivate)
end

function Init()
  TextWidget = Widget:new(TextWidget)
  ImageWidget = Widget:new(ImageWidget)
  FlashWidget = Widget:new(FlashWidget)
  MinimapWidget = Widget:new(MinimapWidget)
  SpriteWidget = ImageWidget:new(SpriteWidget)
  MovieWidget = Widget:new(MovieWidget)
  Widget.BasicData.bTransient = nil
  TextWidget.BasicData.bTransient = nil
  ImageWidget.BasicData.bTransient = nil
  FlashWidget.BasicData.bTransient = nil
  MinimapWidget.BasicData.bTransient = nil
  SpriteWidget.BasicData.bTransient = nil
  MovieWidget.BasicData.bTransient = nil
  if Sys.IsConfirmOnCircle and Sys.IsConfirmOnCircle() then
    local nOldConfirm = Joystick.BUTTON_PAD2_D
    local nOldCancel = Joystick.BUTTON_PAD2_R
    Joystick.BUTTON_PAD2_D = nOldCancel
    Joystick.BUTTON_PAD2_R = nOldConfirm
  end
end

function ReInit()
end

function DeInit()
end
