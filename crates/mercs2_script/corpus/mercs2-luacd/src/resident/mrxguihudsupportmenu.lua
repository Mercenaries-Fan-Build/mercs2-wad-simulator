import("MrxGuiBase")
import("MrxPmc")
import("MrxGuiManager")
import("MrxSupport")
import("MrxSupportManager")
_knFrame = 0.022222223

function AddItem(oSupportMenu, tData)
  local oSupport = tData.oSupport
  if "table" ~= type(oSupport) or not oSupport.GetOwner then
    return
  end
  local oImageWidget = tData.oImageWidget
  if oImageWidget or oSupportMenu.CustomData.bEnabled or oSupportMenu.CustomData.bSuspendInput then
    if oSupportMenu.CustomData.bAddItemInProgress or oSupportMenu.CustomData.bSuspendInput then
      table.insert(oSupportMenu.CustomData.tPendingItemQueue, tData)
      return
    end
    oSupportMenu.CustomData.bAddItemInProgress = true
  end
  if not oSupportMenu.CustomData.bEnabled and tData.bAnimate then
    oSupportMenu.CustomData.oAddAnim:Show(tData.sIcon, tData.sName)
    _AddItemToInternalList(oSupportMenu, tData)
    return
  end
  if Net.IsServer() and not tData.bDontNetSync then
    Net.SendEvent_AddSupportItem(tData.sName or "", tData.sIcon or "", tData.sLitIcon or "", tData.oSupport:GetModuleName())
  end
  _AddItemToInternalList(oSupportMenu, tData)
  if "table" == type(oImageWidget) then
    local oDisplayWidget = oSupportMenu.CustomData.tDisplayList[oSupportMenu.CustomData.nSelectedDisplayIndex].CustomData.oIcon
    local bNeedsClosing = false
    if not oSupportMenu.CustomData.bEnabled then
      oSupportMenu:Open()
      bNeedsClosing = true
    end
    for sEventType in pairs(oImageWidget.EventHandlers) do
      oImageWidget:SetEventHandler(sEventType, nil)
    end
    if oImageWidget:GetOwner() ~= oSupportMenu:GetOwner() then
      oImageWidget:SetOwner(oSupportMenu:GetOwner())
    end
    oImageWidget:SetTranslucency(255)
    oImageWidget:SetAnchoring("left", "center")
    local nStartX1, nStartY1, nStartX2, nStartY2 = oImageWidget:GetLocation()
    local nEndX1, nEndY1, nEndX2, nEndY2 = oDisplayWidget:GetLocation()
    local nTargetPoint = oImageWidget:AddAnimationPoint({
      x = nEndX1,
      y = nEndY1,
      x2 = nEndX2,
      y2 = nEndY2,
      TranslucencyLevel = 0
    })
    oDisplayWidget:SetLocation(nStartX1, nStartY1, nStartX2, nStartY2)
    oDisplayWidget:SetTranslucency(0)
    oImageWidget.CustomData.oParentSupportMenu = oSupportMenu
    oDisplayWidget:AnimateToPoint(oDisplayWidget.CustomData.nOriginalPoint, 1, true)
    oImageWidget:AnimateToPoint(nTargetPoint, 1, true, _RemoveAddAnimationComplete)
  elseif oSupportMenu.CustomData.bEnabled then
    local tDataStuff = {
      1,
      oSupportMenu,
      tData.fCallback,
      tData.tCallbackData
    }
    _PerformAddAnimation(oSupportMenu, _AddAnimationComplete, tDataStuff)
  else
    oSupportMenu:SetVisible(false)
  end
end

function _AddItemToInternalList(oSupportMenu, tData)
  if tData.bAlreadyAdded then
    return
  end
  local oItem = CreateInternalListItem(tData)
  if oSupportMenu.CustomData.nNumberOfItems <= 0 then
    table.insert(oSupportMenu.CustomData.tItemList, oItem)
  else
    if 0 >= oSupportMenu.CustomData.nSelectedItemIndex then
      oSupportMenu.CustomData.nSelectedItemIndex = #oSupportMenu.CustomData.tItemList
    elseif oSupportMenu.CustomData.nSelectedItemIndex > #oSupportMenu.CustomData.tItemList then
      oSupportMenu.CustomData.nSelectedItemIndex = 1
    end
    table.insert(oSupportMenu.CustomData.tItemList, oSupportMenu.CustomData.nSelectedItemIndex, oItem)
  end
  if oSupportMenu.CustomData.nNumberOfItems < 0 then
    oSupportMenu.CustomData.nNumberOfItems = 0
  end
  if not oItem.oSupport:GetOwner() then
    oItem.oSupport:SetOwner(oSupportMenu:GetOwner())
  end
  if oItem.nFuelCost then
    oItem.oSupport:SetFuelCost(oItem.nFuelCost)
  end
  if oItem.nCashCost then
    oItem.oSupport:SetCashCost(oItem.nCashCost)
  end
  oSupportMenu.CustomData.nNumberOfItems = oSupportMenu.CustomData.nNumberOfItems + 1
  if 0 >= oSupportMenu.CustomData.nSelectedItemIndex then
    oSupportMenu.CustomData.nSelectedItemIndex = #oSupportMenu.CustomData.tItemList
  elseif oSupportMenu.CustomData.nSelectedItemIndex > #oSupportMenu.CustomData.tItemList then
    oSupportMenu.CustomData.nSelectedItemIndex = 1
  end
  tData.bAlreadyAdded = true
end

function RemoveItem(oSupportMenu, sItemName, bDontNetSync)
  local nPendingIndex
  for nSearchIndex, oItem in pairs(oSupportMenu.CustomData.tPendingItemQueue) do
    if sItemName == oItem.sName then
      nPendingIndex = nSearchIndex
    end
  end
  if nPendingIndex then
    Debug.Printf("Removing: " .. oSupportMenu.CustomData.tPendingItemQueue[nPendingIndex].sName)
    table.remove(oSupportMenu.CustomData.tPendingItemQueue, nPendingIndex)
  end
  oSupportMenu.CustomData.oAddAnim:Remove(sItemName)
  local nIndex
  for nSearchIndex, oItem in pairs(oSupportMenu.CustomData.tItemList) do
    if sItemName == oItem.sName then
      nIndex = nSearchIndex
    end
  end
  if not nIndex then
    return
  end
  if Net.IsServer() and not bDontNetSync then
    Net.SendEvent_RemoveSupportItem(sItemName)
  end
  table.remove(oSupportMenu.CustomData.tItemList, nIndex)
  oSupportMenu.CustomData.nNumberOfItems = oSupportMenu.CustomData.nNumberOfItems - 1
  if oSupportMenu.CustomData.nSelectedItemIndex > oSupportMenu.CustomData.nNumberOfItems then
    oSupportMenu.CustomData.nSelectedItemIndex = oSupportMenu.CustomData.nNumberOfItems
  end
  oSupportMenu:_SetDisplayInformation()
  if oSupportMenu.CustomData.bEnabled and oSupportMenu.CustomData.nNumberOfItems <= 0 then
    oSupportMenu.CustomData.bSuspendInput = false
    oSupportMenu:Close()
  else
    oSupportMenu:SetVisible(false)
  end
end

function RemoveAll(oSupportMenu)
  oSupportMenu.CustomData.nNumberOfItems = 0
  oSupportMenu.CustomData.nSelectedItemIndex = 1
  oSupportMenu.CustomData.tItemList = {}
  if oSupportMenu.CustomData.bEnabled then
    oSupportMenu:Close()
  end
end

function Trigger(oSupportMenu)
  if oSupportMenu.CustomData.bSuspendInput then
    oSupportMenu.CustomData.bConfirmEntered = true
    return
  end
  local oItem = oSupportMenu.CustomData.tItemList[oSupportMenu.CustomData.nSelectedItemIndex]
  local oSupport = oItem.oSupport
  local nFuelCost = oSupport:GetFuelCost()
  local nFuelRemaining = MrxPmc.GetFuelQty()
  if nFuelCost and nFuelCost > nFuelRemaining and not oSupport.bUnrestrictedByFuel then
    return
  end
  local sSupportName = oSupport:GetSupportName()
  local nFreeCount = MrxPmc.GetFreebieQty(sSupportName)
  if nFreeCount then
    if nFreeCount < 1 then
      local nCashCost = oSupport:GetCashCost()
      if not nCashCost then
        return
      end
      local nCashRemaining = MrxPmc.GetCashQty()
      if nCashCost > nCashRemaining then
        return
      end
    end
  else
    local nSupportRemaining = MrxPmc.GetSupportQty(sSupportName)
    if nSupportRemaining and nSupportRemaining < 1 then
      return
    end
    if oSupportMenu.CustomData.bShootingGalleryMode then
      return
    end
  end
  if oSupport:GetDenialCondition() then
    return
  end
  if not oSupport:ShouldSuppressIconAnimationOnDirectUse() then
    local oGunAmmoCounter = MrxGuiBase.GetWidgetByNameAndOwner("Current Gun", oSupportMenu:GetOwner())
    if oGunAmmoCounter then
      oGunAmmoCounter:SetSuppressAnimation(true)
    end
  end
  if oItem and oItem.fTrigger then
    bSuccess = oItem.fTrigger(unpack(oItem.tCallbackData))
  end
  oSupportMenu:Close()
  oSupportMenu.CustomData.bSuspendInput = true
  if not bSuccess then
    if oGunAmmoCounter then
      oGunAmmoCounter:SetSuppressAnimation(false)
    end
    return
  end
  Sound.CueSound(0, "ui_HUD_Support_Select")
  if not oSupport:ShouldSuppressIconAnimationOnDirectUse() then
    oSupportMenu.CustomData.tDisplayList[3]:SetVisible(false)
    _CreateFlyingIcon(oSupportMenu.CustomData.tDisplayList[3])
  end
end

function Open(oSupportMenu, bAnimateAdd)
  if oSupportMenu.CustomData.bEnabled then
    if oSupportMenu.CustomData.bAnimatingAdd and not bAnimateAdd then
      MrxGuiBase.GetControlFocus(oSupportMenu, false)
      oSupportMenu.CustomData.bAnimatingAdd = false
      oSupportMenu.CustomData.bSnapAddAnimation = true
    end
    return
  end
  if oSupportMenu.CustomData.bSuspendInput then
    return
  end
  local oPda = MrxGuiBase.GetWidgetByNameAndOwner("PDA", oSupportMenu:GetOwner())
  if oPda.CustomData.bActive then
    return
  end
  local oCash = MrxGuiBase.GetWidgetByNameAndOwner("money", oSupportMenu:GetOwner())
  local oFuel = MrxGuiBase.GetWidgetByNameAndOwner("fuel", oSupportMenu:GetOwner())
  if oSupportMenu.CustomData.nNumberOfItems <= 0 then
    oCash:Show(4)
    oFuel:Show(4)
    return
  end
  oSupportMenu.CustomData.oAddAnim:Hide()
  oSupportMenu:SetVisible(true)
  oSupportMenu.CustomData.bEnabled = true
  oSupportMenu.CustomData.oUpArrow:SetVisible(false)
  oSupportMenu.CustomData.oDownArrow:SetVisible(false)
  oSupportMenu.CustomData.nBufferedInput = 0
  oSupportMenu.CustomData.bConfirmEntered = nil
  if oCash then
    oCash:Show(-1)
  end
  if oFuel then
    oFuel:Show(-1)
  end
  oSupportMenu:SetLocation()
  oSupportMenu:_SetDisplayInformation()
  if bAnimateAdd then
    oSupportMenu.CustomData.bAnimatingAdd = true
  else
    oSupportMenu.CustomData.bAnimatingAdd = false
    MrxGuiBase.GetControlFocus(oSupportMenu, false)
    Event.Post("Support Menu Open", {
      uPlayer = oSupportMenu:GetOwner()
    })
  end
  local tDisplay = oSupportMenu.CustomData.tDisplayList
  tDisplay[2]:SetupOpen()
  tDisplay[3]:SetupOpen()
  tDisplay[4]:SetupOpen()
  oSupportMenu.CustomData.oDescripters:SetVisible(false)
  tDisplay[2].CustomData.oOrbit:AnimateToPoint(tDisplay[2].CustomData.oOrbit.CustomData.nFadeInPoint, _knFrame * 10, false)
  local oBulletRing = oSupportMenu.CustomData.oBulletRing
  oBulletRing:SetTranslucency(0)
  oBulletRing:AnimateToPoint(oBulletRing.CustomData.nFadeInPoint, _knFrame * 15, true)
  local i = 1
  while i <= 5 do
    oBulletRing:AnimateBullet(i, _knFrame * 15, 270, 0, 1, false)
    i = i + 1
  end
  oSupportMenu.CustomData.oUpArrowBg:SetVisible(false)
  oSupportMenu.CustomData.oDownArrowBg:SetVisible(false)
  local oFrame = oSupportMenu.CustomData.oFrame
  _FrameClosed(oFrame)
  oFrame:SetVisible(false)
  oFrame:AnimateToPoint(oFrame.CustomData.nClosePoint, 0, true)
  oSupportMenu.CustomData.bSuspendInput = true
  oSupportMenu.CustomData.nTime = 0
  oSupportMenu:SetEventHandler("GuiUpdate", HandleUpdateForOpen)
  oSupportMenu.CustomData.bCloseOnComplete = false
  oSupportMenu.CustomData.oClockIcon:SetVisible(false)
  Sound.CueSound(0, "ui_HUD_Support_Open_Menu")
end

function Close(oSupportMenu)
  if not oSupportMenu.CustomData.bEnabled then
    return
  end
  MrxGuiBase.ReleaseControlFocus(oSupportMenu)
  if oSupportMenu.CustomData.bSuspendInput then
    oSupportMenu.CustomData.bCloseOnComplete = true
    return
  end
  oSupportMenu.CustomData.bEnabled = false
  local oCash = MrxGuiBase.GetWidgetByNameAndOwner("money", oSupportMenu:GetOwner())
  local oFuel = MrxGuiBase.GetWidgetByNameAndOwner("fuel", oSupportMenu:GetOwner())
  if oCash then
    oCash:Hide()
  end
  if oFuel then
    oFuel:Hide()
  end
  for nIndex, tItem in pairs(oSupportMenu.CustomData.tPendingItemQueue) do
    oSupportMenu:AddItem({
      sName = tItem.sName,
      sIcon = tItem.sIcon,
      sLitIcon = tItem.sLitIcon,
      oSupport = tItem.oSupport,
      bAlreadyAdded = tItem.bAlreadyAdded
    })
    if tItem.fCallback then
      tItem.fCallback(unpack(tItem.tCallbackData or {}))
    end
  end
  oSupportMenu.CustomData.tPendingItemQueue = {}
  oSupportMenu.CustomData.bAddItemInProgress = false
  oSupportMenu.CustomData.bSuspendInput = false
  oSupportMenu.CustomData.nTime = 0
  oSupportMenu:SetEventHandler("GuiUpdate", HandleUpdateForClose)
  Sound.CueSound(0, "ui_HUD_Support_Close_Menu")
  Event.Post("Support Menu Close", {
    uPlayer = oSupportMenu:GetOwner()
  })
  oSupportMenu.CustomData.oClockIcon:SetVisible(false)
end

function SetCash(oSupportMenu, nCash)
end

function SetFuel(oSupportMenu, nFuel)
end

function SetShootingGalleryMode(oSupportMenu, bEnabled)
  oSupportMenu.CustomData.bShootingGalleryMode = bEnabled
end

function HandleInitializationEvent(oWidget, tEvent)
  local tChildren = oWidget:GetChildren()
  oWidget.CustomData.nMaxDisplayItems = table.getn(tChildren[2]:GetChildren())
  local oDisplay = tChildren[2]:GetChildren()
  oDisplay[1]:SetTranslucency(0)
  oDisplay[oWidget.CustomData.nMaxDisplayItems]:SetTranslucency(0)
  local nDisplayX, nDisplayY, nDisplayX2 = oDisplay[1]:GetLocation()
  local nIndex = 1
  while nIndex <= oWidget.CustomData.nMaxDisplayItems do
    local nX1, nY1, nX2 = oDisplay[nIndex]:GetLocation()
    oDisplay[nIndex].CustomData.nAddPoint = oDisplay[nIndex]:AddAnimationPoint({x = nX1, x2 = nX2})
    oDisplay[nIndex].CustomData.oBackground = oDisplay[nIndex]:GetChildren()[1]
    oDisplay[nIndex].CustomData.oIcon = oDisplay[nIndex]:GetChildren()[2]
    nIndex = nIndex + 1
  end
  nIndex = 1
  local nTranslucency = 255
  while nIndex <= oWidget.CustomData.nMaxDisplayItems do
    _SetupItemAnimationPoints(nIndex, oDisplay)
    nIndex = nIndex + 1
  end
  local oItem
  nIndex = 1
  while nIndex <= oWidget.CustomData.nMaxDisplayItems do
    oItem = oDisplay[nIndex]
    MrxGuiBase.PushWidgetToFront(oItem.CustomData.oIcon)
    nIndex = nIndex + 1
  end
  nIndex = 1
  while nIndex <= oWidget.CustomData.nMaxDisplayItems do
    oItem = oDisplay[nIndex]
    MrxGuiBase.PushWidgetToFront(oItem.CustomData.oStatus)
    nIndex = nIndex + 1
  end
  oWidget.CustomData.tDisplayList = oDisplay
  oWidget.CustomData.oCursor = tChildren[3]
  oWidget.CustomData.nSelectedDisplayIndex = 3
  oWidget.CustomData.nSelectedItemIndex = 0
  oWidget.CustomData.nNumberOfItems = 0
  oWidget.CustomData.tItemList = {}
  oWidget.CustomData.tPendingItemQueue = {}
  oWidget.CustomData.bAddItemInProgress = false
  oWidget.CustomData.nItemSpacing = 43
  oWidget.CustomData.nDisplayX, oWidget.CustomData.nDisplayY = tChildren[2]:GetLocation()
  oWidget.CustomData.nDisplayPoint = tChildren[2]:AddAnimationPoint({
    x = oWidget.CustomData.nDisplayX,
    y = oWidget.CustomData.nDisplayY
  })
  oWidget.CustomData.nBufferedInput = 0
  oWidget.CustomData.bConfirmEntered = nil
  oWidget._SetDisplayInformation = _SetDisplayInformation
  oWidget._ScrollUp = _ScrollUp
  oWidget._ScrollDown = _ScrollDown
  oWidget:_SetDisplayInformation()
  oWidget.SetCash = SetCash
  oWidget.SetFuel = SetFuel
  oWidget.Open = Open
  oWidget.Close = Close
  oWidget.Trigger = Trigger
  oWidget.AddItem = AddItem
  oWidget.RemoveItem = RemoveItem
  oWidget.RemoveAll = RemoveAll
  oWidget.SetShootingGalleryMode = SetShootingGalleryMode
  oWidget:SetEventHandler("ControllerInput", HandleInputEvent)
  oWidget:SetEventHandler("GuiGameStateChange", _HandleGameStateChangeEvent)
  oWidget:SetVisible(false)
  oWidget.CustomData.bEnabled = false
  local tBackdropChildren = tChildren[1]:GetChildren()[1]:GetChildren()
  local oUpArrow = tBackdropChildren[2]:GetChildren()[1]
  local oDownArrow = tBackdropChildren[3]:GetChildren()[1]
  oUpArrow.CustomData.nFadeInPoint = oUpArrow:AddAnimationPoint({TranslucencyLevel = 255})
  oUpArrow.CustomData.nFadeOutPoint = oUpArrow:AddAnimationPoint({TranslucencyLevel = 0})
  oDownArrow.CustomData.nFadeInPoint = oDownArrow:AddAnimationPoint({TranslucencyLevel = 255})
  oDownArrow.CustomData.nFadeOutPoint = oDownArrow:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.oUpArrow = oUpArrow
  oWidget.CustomData.oDownArrow = oDownArrow
  local oUpArrowBg = tBackdropChildren[2]
  local oDownArrowBg = tBackdropChildren[3]
  oUpArrowBg.CustomData.nFadeInPoint = oUpArrowBg:AddAnimationPoint({TranslucencyLevel = 255})
  oUpArrowBg.CustomData.nFadeOutPoint = oUpArrowBg:AddAnimationPoint({TranslucencyLevel = 0})
  oDownArrowBg.CustomData.nFadeInPoint = oDownArrowBg:AddAnimationPoint({TranslucencyLevel = 255})
  oDownArrowBg.CustomData.nFadeOutPoint = oDownArrowBg:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.oUpArrowBg = oUpArrowBg
  oWidget.CustomData.oDownArrowBg = oDownArrowBg
  local oFrame = tBackdropChildren[1]
  local nFrameX1, nFrameY1, nFrameX2, nFrameY2 = oFrame:GetLocation()
  oFrame.CustomData.tFramePieces = oFrame:GetChildren()
  oFrame.CustomData.nOriginPoint = oFrame:AddAnimationPoint({y = nFrameY1, y2 = nFrameY2})
  oFrame.CustomData.nClosePoint = oFrame:AddAnimationPoint({
    y = (nFrameY1 + nFrameY2) / 2,
    y2 = (nFrameY1 + nFrameY2) / 2
  })
  oWidget.CustomData.oFrame = oFrame
  local oBulletRing = tChildren[3]:GetChildren()[2]
  oWidget.CustomData.oBulletRing = oBulletRing
  _InitializeBullets(oBulletRing)
  oWidget.CustomData.oDescripters = tChildren[3]:GetChildren()[1]
  local tDescriptorChildren = oWidget.CustomData.oDescripters:GetChildren()
  oWidget.CustomData.oSupportNameText = tDescriptorChildren[1]
  oWidget.CustomData.oFuelCostIcon = tDescriptorChildren[2]
  oWidget.CustomData.oFuelCostText = tDescriptorChildren[3]
  oWidget.CustomData.oStockpileText = tDescriptorChildren[4]
  oWidget.CustomData.oDesignator = tDescriptorChildren[5]
  oFrame.CustomData.oSupportNameText = oWidget.CustomData.oSupportNameText
  oWidget.CustomData.oClockIcon = tChildren[3]:GetChildren()[3]
  oWidget.CustomData.oClockIcon:SetVisible(false)
  local oAddAnimation = InitAddWidget(51, 246, 47, oWidget:GetOwner())
  MrxGuiManager.AddWidgetToHud(oWidget:GetOwner(), oAddAnimation, true)
  oWidget.CustomData.oAddAnim = oAddAnimation
end

function _SetDisplayInformation(oWidget)
  if oWidget.CustomData.nNumberOfItems < 1 then
    local nIndex = 1
    while nIndex <= 5 do
      oWidget.CustomData.tDisplayList[nIndex].CustomData.oIcon:SetVisible(false)
      nIndex = nIndex + 1
    end
    return
  end
  local nIndex = 1
  while nIndex <= 5 do
    oWidget.CustomData.tDisplayList[nIndex].CustomData.oIcon:SetVisible(true)
    nIndex = nIndex + 1
  end
  local tDisplayList = oWidget.CustomData.tDisplayList
  local tItemList = oWidget.CustomData.tItemList
  local nItemIndex = oWidget.CustomData.nSelectedItemIndex - math.floor(oWidget.CustomData.nMaxDisplayItems / 2)
  local nRealIndex
  local nDisplayIndex = 1
  while nDisplayIndex <= #tDisplayList do
    nRealIndex = WrapIndex(nItemIndex, oWidget.CustomData.nNumberOfItems)
    if tItemList[nRealIndex].sIcon then
      tDisplayList[nDisplayIndex].CustomData.oIcon:SetTexture(tItemList[nRealIndex].sIcon)
      tDisplayList[nDisplayIndex].CustomData.oIcon:SetVisible(true)
    else
      tDisplayList[nDisplayIndex].CustomData.oIcon:SetVisible(false)
    end
    tDisplayList[nDisplayIndex].CustomData.sIcon = tItemList[nRealIndex].sIcon
    tDisplayList[nDisplayIndex].CustomData.sLitIcon = tItemList[nRealIndex].sLitIcon
    local oStatus = tDisplayList[nDisplayIndex].CustomData.oStatus
    if tItemList[nRealIndex].oSupport then
      local sDenial
      local nStock = MrxPmc.GetSupportQty(tItemList[nRealIndex].oSupport:GetSupportName())
      local nFreebies = MrxPmc.GetFreebieQty(tItemList[nRealIndex].oSupport:GetSupportName())
      if not nFreebies and oWidget.CustomData.bShootingGalleryMode then
        sDenial = "disabled"
      elseif tItemList[nRealIndex].oSupport:GetFuelCost() > MrxPmc.GetFuelQty() and not tItemList[nRealIndex].oSupport.bUnrestrictedByFuel then
        sDenial = "fuel"
      elseif nStock and nStock < 1 then
        sDenial = "zero"
      else
        sDenial = tItemList[nRealIndex].oSupport:GetDenialCondition()
      end
      tDisplayList[nDisplayIndex]:SetStatus(sDenial)
    else
      tDisplayList[nDisplayIndex]:SetStatus(nil)
    end
    nDisplayIndex = nDisplayIndex + 1
    nItemIndex = nItemIndex + 1
  end
  local oCursor = oWidget.CustomData.oCursor
  oCursor:SetVisible(true)
  UpdateDisplayText(oWidget)
end

function _ScrollDown(oWidget)
  if oWidget.CustomData.nNumberOfItems <= 1 then
    return
  end
  if oWidget.CustomData.bSuspendInput then
    if not oWidget.CustomData.bConfirmEntered then
      oWidget.CustomData.nBufferedInput = oWidget.CustomData.nBufferedInput - 1
    end
    return
  end
  oWidget.CustomData.bSuspendInput = true
  oWidget.CustomData.nSelectedItemIndex = WrapIndex(oWidget.CustomData.nSelectedItemIndex + 1, oWidget.CustomData.nNumberOfItems)
  oWidget:_SetDisplayInformation()
  local tDisplay = oWidget.CustomData.tDisplayList
  tDisplay[1]:SetVisible(true)
  tDisplay[2]:SetVisible(true)
  tDisplay[3]:SetVisible(true)
  tDisplay[4]:SetVisible(true)
  tDisplay[1]:SetTranslucency(255)
  tDisplay[2]:SetTranslucency(255)
  tDisplay[3]:SetTranslucency(255)
  tDisplay[4]:SetTranslucency(255)
  _HaltStatusPulse(tDisplay[1].CustomData.oStatus, true)
  _HaltStatusPulse(tDisplay[2].CustomData.oStatus, true)
  _HaltStatusPulse(tDisplay[3].CustomData.oStatus, true)
  _HaltStatusPulse(tDisplay[4].CustomData.oStatus, true)
  _SetupSquish(tDisplay[4], true, false)
  _SetupItemPrev(tDisplay[3])
  _SetupItemPrev(tDisplay[2])
  _SetupSquish(tDisplay[1], false, true)
  oWidget.CustomData.oDescripters:SetVisible(false)
  local oFrame = oWidget.CustomData.oFrame
  oFrame:AnimateToPoint(oFrame.CustomData.nClosePoint, _knFrame * 5, true, _FrameClosed, {})
  local oArrow = oWidget.CustomData.oUpArrow
  oArrow:SetVisible(true)
  oArrow:AnimateToPoint(oArrow.CustomData.nFadeInPoint, 0, true)
  oArrow:AnimateToPoint(oArrow.CustomData.nFadeOutPoint, _knFrame * 5, false)
  oWidget.CustomData.nTime = 0
  oWidget:SetEventHandler("GuiUpdate", HandleUpdateForTriggerDown)
  oWidget.CustomData.nAnimatingWidgets = 0
  oWidget.CustomData.oClockIcon:SetVisible(false)
  Sound.CueSound(0, "ui_HUD_Support_Scroll")
end

function _ScrollUp(oWidget)
  if oWidget.CustomData.nNumberOfItems <= 1 then
    return
  end
  if oWidget.CustomData.bSuspendInput then
    if not oWidget.CustomData.bConfirmEntered then
      oWidget.CustomData.nBufferedInput = oWidget.CustomData.nBufferedInput + 1
    end
    return
  end
  oWidget.CustomData.bSuspendInput = true
  oWidget.CustomData.nSelectedItemIndex = WrapIndex(oWidget.CustomData.nSelectedItemIndex - 1, oWidget.CustomData.nNumberOfItems)
  oWidget:_SetDisplayInformation()
  local tDisplay = oWidget.CustomData.tDisplayList
  tDisplay[2]:SetVisible(true)
  tDisplay[3]:SetVisible(true)
  tDisplay[4]:SetVisible(true)
  tDisplay[5]:SetVisible(true)
  tDisplay[2]:SetTranslucency(255)
  tDisplay[3]:SetTranslucency(255)
  tDisplay[4]:SetTranslucency(255)
  tDisplay[5]:SetTranslucency(255)
  _HaltStatusPulse(tDisplay[2].CustomData.oStatus, true)
  _HaltStatusPulse(tDisplay[3].CustomData.oStatus, true)
  _HaltStatusPulse(tDisplay[4].CustomData.oStatus, true)
  _HaltStatusPulse(tDisplay[5].CustomData.oStatus, true)
  _SetupSquish(tDisplay[2], true, true)
  _SetupItemNext(tDisplay[3])
  _SetupItemNext(tDisplay[4])
  _SetupSquish(tDisplay[5], false, false)
  oWidget.CustomData.oDescripters:SetVisible(false)
  local oFrame = oWidget.CustomData.oFrame
  oFrame:AnimateToPoint(oFrame.CustomData.nClosePoint, _knFrame * 5, true, _FrameClosed, {})
  local oArrow = oWidget.CustomData.oDownArrow
  oArrow:SetVisible(true)
  oArrow:AnimateToPoint(oArrow.CustomData.nFadeInPoint, 0, true)
  oArrow:AnimateToPoint(oArrow.CustomData.nFadeOutPoint, _knFrame * 5, false)
  oWidget.CustomData.nTime = 0
  oWidget:SetEventHandler("GuiUpdate", HandleUpdateForTriggerUp)
  oWidget.CustomData.nAnimatingWidgets = 0
  oWidget.CustomData.oClockIcon:SetVisible(false)
  Sound.CueSound(0, "ui_HUD_Support_Scroll")
end

function CreateInternalListItem(tData)
  local tNew = {}
  for key, value in pairs(tData) do
    tNew[key] = value
  end
  tNew.sName = ValidateParameter(tNew.sName, "string", "Unnamed")
  if Net.IsClient() then
    if type(tNew.sIcon) ~= "string" and type(tNew.sIcon) ~= "userdata" then
      tNew.sIcon = "HUD_ICON_support_crate"
    end
    if type(tNew.sLitIcon) ~= "string" and type(tNew.sLitIcon) ~= "userdata" then
      tNew.sLitIcon = tNew.sIcon
    end
  else
    tNew.sIcon = ValidateParameter(tNew.sIcon, "string", "HUD_ICON_support_crate")
    tNew.sLitIcon = ValidateParameter(tNew.sLitIcon, "string", tNew.sIcon)
  end
  tNew.oSupport = ValidateParameter(tNew.oSupport, "table", nil)
  tNew.fTrigger = TriggerItem
  tNew.tCallbackData = {
    tNew.oSupport
  }
  return tNew
end

function TriggerItem(oSupport)
  if not (oSupport and oSupport.Create) or not oSupport.Commence then
    return
  end
  local oNewSupport = oSupport:Create(oSupport.uOwner)
  oNewSupport:SetSupportName(oSupport:GetSupportName())
  oNewSupport:SetFuelCost(oSupport:GetFuelCost())
  oNewSupport:SetCashCost(oSupport:GetCashCost())
  return oNewSupport:Commence(true)
end

function UpdateDisplayText(oWidget, bHalt)
  if not oWidget.CustomData.bEnabled then
    return
  end
  _UpdateDisplayedText(oWidget)
  _UpdateClockIcon(oWidget, bHalt)
end

function _FixText(oWidget)
  Event.Create(Event.TimerRelative, {0.1}, UpdateDisplayText, {oWidget, true})
end

function _UpdateDisplayedText(oWidget)
  local tData = oWidget.CustomData.tItemList[oWidget.CustomData.nSelectedItemIndex]
  ASSERT(tData)
  local oSupport = tData.oSupport
  oWidget.CustomData.oSupportNameText:SetText(tData.sName)
  local sDenialCondition = oSupport and oSupport:GetDenialCondition()
  if sDenialCondition and ("[pda.support.denied.rearming]" == string.lower(sDenialCondition) or "[Generic.Attitudes.Hostile]" == sDenialCondition) then
    oWidget.CustomData.oFuelCostIcon:SetTranslucency(0)
    oWidget.CustomData.oFuelCostText:SetTranslucency(0)
    oWidget.CustomData.oStockpileText:SetColor(255, 64, 64)
    oWidget.CustomData.oStockpileText:SetText(sDenialCondition)
    oWidget.CustomData.oDesignator:SetText("")
  else
    local sSupportName = oSupport:GetSupportName()
    local nFreeCount = MrxPmc.GetFreebieQty(sSupportName)
    if nFreeCount then
      if nFreeCount < 1 then
        local nCashCost = oSupport:GetCashCost()
        if nCashCost then
          oWidget.CustomData.oStockpileText:SetText("$" .. nCashCost)
          if nCashCost > MrxPmc.GetCashQty() then
            oWidget.CustomData.oStockpileText:SetColor(255, 64, 64)
          else
            oWidget.CustomData.oStockpileText:SetColor(255, 255, 255)
          end
        else
          oWidget.CustomData.oStockpileText:SetText("[Generic.SupportQtyDepleted]")
          oWidget.CustomData.oStockpileText:SetColor(255, 64, 64)
        end
      else
        local sText = "[green]" .. nFreeCount .. " [Generic.SupportQtyFreeSuffix]"
        oWidget.CustomData.oStockpileText:SetText(sText)
        oWidget.CustomData.oStockpileText:SetColor(255, 255, 255)
      end
    else
      local nStockCount = MrxPmc.GetSupportQty(sSupportName)
      if nStockCount then
        oWidget.CustomData.oStockpileText:SetText("[Generic.SupportQtyPrefix] " .. nStockCount)
        if nStockCount < 1 then
          oWidget.CustomData.oStockpileText:SetColor(255, 64, 64)
        else
          oWidget.CustomData.oStockpileText:SetColor(255, 255, 255)
        end
      else
        oWidget.CustomData.oStockpileText:SetText("")
      end
    end
    local nFuelCost = oSupport:GetFuelCost()
    if not nFuelCost or nFuelCost == 0 then
      oWidget.CustomData.oFuelCostText:SetTranslucency(0)
      oWidget.CustomData.oFuelCostIcon:SetTranslucency(0)
    else
      oWidget.CustomData.oFuelCostIcon:SetTranslucency(255)
      oWidget.CustomData.oFuelCostText:SetTranslucency(255)
      oWidget.CustomData.oFuelCostText:SetText(nFuelCost)
      if nFuelCost > MrxPmc.GetFuelQty() and not oSupport.bUnrestrictedByFuel then
        oWidget.CustomData.oFuelCostText:SetColor(255, 64, 64)
        oWidget.CustomData.oFuelCostIcon:SetColor(255, 64, 64)
      else
        oWidget.CustomData.oFuelCostText:SetColor(255, 255, 255)
        oWidget.CustomData.oFuelCostIcon:SetColor(255, 255, 255)
      end
    end
    local sDisplay = " "
    if oSupport:GetDesignator() then
      local sDesignator = oSupport:GetDesignator():GetType()
      if "smoke" == sDesignator then
        sDisplay = "[Generic.SupportDesignators.Smoke]"
      elseif "satellite" == sDesignator then
        sDisplay = "[Generic.SupportDesignators.Satellite]"
      elseif "advanced satellite" == sDesignator then
        sDisplay = "[Generic.SupportDesignators.AdvSatellite]"
      elseif "beacon" == sDesignator then
        sDisplay = "[Generic.SupportDesignators.Beacon]"
      elseif "laser" == sDesignator then
        sDisplay = "[Generic.SupportDesignators.Laser]"
      elseif "flare" == sDesignator then
        sDisplay = "[Generic.SupportDesignators.Flare]"
      end
    end
    oWidget.CustomData.oDesignator:SetText(sDisplay)
  end
end

function _UpdateClockIcon(oWidget, bHalt)
  local tData = oWidget.CustomData.tItemList[oWidget.CustomData.nSelectedItemIndex]
  ASSERT(tData)
  local oSupport = tData.oSupport
  oWidget.CustomData.oClockIcon:SetVisible(false)
  if oSupport and oSupport.GetElapsedCooldownTime then
    local nElapsedTime, nTotalTime = oSupport:GetElapsedCooldownTime()
    if nElapsedTime and nTotalTime and nElapsedTime < nTotalTime then
      oWidget.CustomData.oClockIcon:SetVisible(true)
      if 0 < nTotalTime then
        oWidget.CustomData.oClockIcon:SetClockAnimation(nElapsedTime, nTotalTime)
        if not bHalt then
          oWidget.CustomData.oClockIcon:SetClockAnimationCallback(_FixText, {oWidget})
        end
      end
    else
      oWidget.CustomData.oClockIcon:SetClockAnimationCallback(nil, nil)
    end
  else
    oWidget.CustomData.oClockIcon:SetClockAnimationCallback(nil, nil)
  end
end

function _UpdateStatusDisplay(oSupportMenu)
  local tDisplayList = oSupportMenu.CustomData.tDisplayList
  local tItemList = oSupportMenu.CustomData.tItemList
  local nItemIndex = oSupportMenu.CustomData.nSelectedItemIndex - math.floor(oSupportMenu.CustomData.nMaxDisplayItems / 2)
  local nRealIndex
  local nDisplayIndex = 1
  while nDisplayIndex <= #tDisplayList do
    nRealIndex = WrapIndex(nItemIndex, oSupportMenu.CustomData.nNumberOfItems)
    local oStatus = tDisplayList[nDisplayIndex].CustomData.oStatus
    if tItemList[nRealIndex].oSupport then
      local sDenial
      local nStock = MrxPmc.GetSupportQty(tItemList[nRealIndex].oSupport:GetSupportName())
      local nFreebies = MrxPmc.GetFreebieQty(tItemList[nRealIndex].oSupport:GetSupportName())
      if not nFreebies and oSupportMenu.CustomData.bShootingGalleryMode then
        sDenial = "disabled"
      elseif tItemList[nRealIndex].oSupport:GetFuelCost() > MrxPmc.GetFuelQty() and not tItemList[nRealIndex].oSupport.bUnrestrictedByFuel then
        sDenial = "fuel"
      elseif nStock and nStock < 1 then
        sDenial = "zero"
      else
        sDenial = tItemList[nRealIndex].oSupport:GetDenialCondition()
      end
      tDisplayList[nDisplayIndex]:SetStatus(sDenial)
    else
      tDisplayList[nDisplayIndex]:SetStatus(nil)
    end
    nDisplayIndex = nDisplayIndex + 1
    nItemIndex = nItemIndex + 1
  end
end

function _UpdateDisplayPeriodic(oSupportMenu)
  _UpdateDisplayedText(oSupportMenu)
  _UpdateStatusDisplay(oSupportMenu)
  local tData = oSupportMenu.CustomData.tItemList[oSupportMenu.CustomData.nSelectedItemIndex]
  ASSERT(tData)
  local oSupport = tData.oSupport
  if not oSupportMenu.CustomData.oClockIcon:GetVisible() then
    _UpdateClockIcon(oSupportMenu, false)
  end
end

function HandleUpdateForIdle(oWidget, nDeltaTime)
  if not oWidget.CustomData.nIdleTime then
    oWidget.CustomData.nIdleTime = 0
  end
  oWidget.CustomData.nIdleTime = nDeltaTime + oWidget.CustomData.nIdleTime
  if oWidget.CustomData.nIdleTime > 0.5 then
    _UpdateDisplayPeriodic(oWidget)
    oWidget.CustomData.nIdleTime = 0
  end
end

function _FrameClosed(oFrame)
  local nEndX = oFrame.CustomData.oSupportNameText:GetLocation()
  nEndX = nEndX + math.max(oFrame.CustomData.oSupportNameText:GetWidth(), 140)
  local nX = oFrame.CustomData.tFramePieces[2]:GetLocation()
  oFrame.CustomData.tFramePieces[2]:SetLocation(nX, nil, nEndX)
  local nY
  nX, nY = oFrame.CustomData.tFramePieces[3]:GetLocation()
  oFrame.CustomData.tFramePieces[3]:SetLocation(nEndX, nY)
  oFrame:SetVisible(false)
end

function HandleUpdateForTriggerUp(oWidget, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local nFrame = _knFrame
  if not oWidget.CustomData.nTime then
    oWidget.CustomData.nTime = 0
  end
  local nPrevTime = oWidget.CustomData.nTime
  local nNewTime = nPrevTime + nDeltaTime
  oWidget.CustomData.nTime = nNewTime
  local tDisplay = oWidget.CustomData.tDisplayList
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 4) then
    local nOverflowTime = math.max(0, nNewTime - nFrame * 4)
    tDisplay[2]:TriggerSquish(nFrame * 10, true, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 5) then
    tDisplay[3]:TriggerNext(nFrame * 10)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 6) then
    tDisplay[4]:TriggerNext(nFrame * 10)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 7) then
    tDisplay[5]:TriggerSquish(nFrame * 10, false, false)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 12) then
    local oFrame = oWidget.CustomData.oFrame
    oFrame:SetVisible(true)
    oFrame:AnimateToPoint(oFrame.CustomData.nOriginPoint, nFrame * 5, true, _FrameAnimationCompleteCallback, {oWidget})
    oWidget.CustomData.nTime = 0
    oWidget:SetEventHandler("GuiUpdate", HandleUpdateForIdle)
  end
end

function HandleUpdateForTriggerDown(oWidget, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local nFrame = _knFrame
  if not oWidget.CustomData.nTime then
    oWidget.CustomData.nTime = 0
  end
  local nPrevTime = oWidget.CustomData.nTime
  local nNewTime = nPrevTime + nDeltaTime
  oWidget.CustomData.nTime = nNewTime
  local tDisplay = oWidget.CustomData.tDisplayList
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 4) then
    tDisplay[4]:TriggerSquish(nFrame * 10, true, false)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 5) then
    tDisplay[3]:TriggerPrev(nFrame * 10)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 6) then
    tDisplay[2]:TriggerPrev(nFrame * 10)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 7) then
    tDisplay[1]:TriggerSquish(nFrame * 10, false, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 12) then
    local oFrame = oWidget.CustomData.oFrame
    oFrame:SetVisible(true)
    oFrame:AnimateToPoint(oFrame.CustomData.nOriginPoint, nFrame * 5, true, _FrameAnimationCompleteCallback, {oWidget})
    oWidget.CustomData.nTime = 0
    oWidget:SetEventHandler("GuiUpdate", HandleUpdateForIdle)
    if 0 < #oWidget.CustomData.tPendingItemQueue then
      _AddAnimationComplete(nil, oWidget)
    end
  end
end

function HandleUpdateForOpen(oWidget, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local nFrame = 0.016666668
  if not oWidget.CustomData.nTime then
    oWidget.CustomData.nTime = 0
  end
  local nPrevTime = oWidget.CustomData.nTime
  local nNewTime = nPrevTime + nDeltaTime
  oWidget.CustomData.nTime = nNewTime
  local tDisplay = oWidget.CustomData.tDisplayList
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 3) then
    tDisplay[3].CustomData.oOrbit:AnimateToPoint(tDisplay[3].CustomData.oOrbit.CustomData.nFadeInPoint, nFrame * 10, false)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 5) then
    tDisplay[2]:TriggerOpen(nFrame * 5)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 6) then
    tDisplay[4].CustomData.oOrbit:AnimateToPoint(tDisplay[4].CustomData.oOrbit.CustomData.nFadeInPoint, nFrame * 10, false)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 8) then
    tDisplay[3]:TriggerOpen(nFrame * 5)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 11) then
    tDisplay[4]:TriggerOpen(nFrame * 5)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 13) then
    local oFrame = oWidget.CustomData.oFrame
    oFrame:SetVisible(true)
    oFrame:AnimateToPoint(oFrame.CustomData.nOriginPoint, nFrame * 5, true, _SetVisible, {
      oWidget.CustomData.oDescripters,
      true
    })
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 20) then
    oWidget.CustomData.nTime = 0
    oWidget:SetEventHandler("GuiUpdate", HandleUpdateForIdle)
    if oWidget.CustomData.nNumberOfItems > 1 then
      local oUpArrowBg = oWidget.CustomData.oUpArrowBg
      local oDownArrowBg = oWidget.CustomData.oDownArrowBg
      oUpArrowBg:SetTranslucency(0)
      oDownArrowBg:SetTranslucency(0)
      oUpArrowBg:SetVisible(true)
      oDownArrowBg:SetVisible(true)
      oUpArrowBg:GetChildren()[1]:SetVisible(false)
      oDownArrowBg:GetChildren()[1]:SetVisible(false)
      oUpArrowBg:AnimateToPoint(oUpArrowBg.CustomData.nFadeInPoint, nFrame * 5, true)
      oDownArrowBg:AnimateToPoint(oDownArrowBg.CustomData.nFadeInPoint, nFrame * 5, true, _AnimationCompleteCallback, {oWidget})
    else
      _AnimationCompleteCallback(oWidget, oWidget)
    end
  end
end

function HandleUpdateForClose(oWidget, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local nFrame = _knFrame
  if not oWidget.CustomData.nTime then
    oWidget.CustomData.nTime = 0
  end
  local nPrevTime = oWidget.CustomData.nTime
  local nNewTime = nPrevTime + nDeltaTime
  oWidget.CustomData.nTime = nNewTime
  local tDisplay = oWidget.CustomData.tDisplayList
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 1) then
    oWidget.CustomData.oBulletRing:AnimateBullet(1, nFrame * 15, 0, 180, 1, true)
    local oUpArrowBg = oWidget.CustomData.oUpArrowBg
    local oDownArrowBg = oWidget.CustomData.oDownArrowBg
    oUpArrowBg:AnimateToPoint(oUpArrowBg.CustomData.nFadeOutPoint, nFrame * 5, true)
    oDownArrowBg:AnimateToPoint(oDownArrowBg.CustomData.nFadeOutPoint, nFrame * 5, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 2) then
    oWidget.CustomData.oBulletRing:AnimateBullet(2, nFrame * 15, 0, 180, 1, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 3) then
    oWidget.CustomData.oBulletRing:AnimateBullet(3, nFrame * 15, 0, 180, 1, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 4) then
    oWidget.CustomData.oBulletRing:AnimateBullet(4, nFrame * 15, 0, 180, 1, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 5) then
    oWidget.CustomData.oBulletRing:AnimateBullet(5, nFrame * 15, 0, 180, 1, true)
    oWidget.CustomData.oDescripters:SetVisible(false)
    local oFrame = oWidget.CustomData.oFrame
    oFrame:AnimateToPoint(oFrame.CustomData.nClosePoint, nFrame * 5, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 7) then
    tDisplay[2]:TriggerClose(nFrame * 5)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 9) then
    tDisplay[3]:TriggerClose(nFrame * 5)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 11) then
    tDisplay[4]:TriggerClose(nFrame * 5)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 22) then
    oWidget:SetVisible(false)
    oWidget:SetEventHandler("GuiUpdate", nil)
    oWidget.CustomData.nTime = 0
    oWidget.CustomData.bSuspendInput = false
    if 0 < #oWidget.CustomData.tPendingItemQueue then
      _AddAnimationComplete(nil, oWidget)
    end
  end
end

function _PassedPoint(nPreviousValue, nNewValue, nPoint)
  if nPreviousValue < nNewValue then
    if nPreviousValue < nPoint and nPoint <= nNewValue then
      return true
    end
  elseif nNewValue < nPreviousValue and nPoint < nPreviousValue and nNewValue <= nPoint then
    return true
  end
  return false
end

function _SetVisible(oUnused, oWidget, bVisible)
  oWidget:SetVisible(bVisible)
end

function _SetupItemAnimationPoints(nIndex, tDisplayList)
  local oWidget = tDisplayList[nIndex]
  local tData = oWidget.CustomData
  local tChildren = oWidget:GetChildren()
  local oIconBg = tChildren[1]
  local oIcon = tChildren[2]
  local oBorder = tChildren[3]
  local oOrbit = tChildren[4]
  local oStatus = tChildren[5]
  tData.oIconBg = oIconBg
  tData.oIcon = oIcon
  tData.oBorder = oBorder
  tData.oOrbit = oOrbit
  tData.oStatus = oStatus
  if tDisplayList[nIndex + 1] then
    local oNext = tDisplayList[nIndex + 1]
    local nNextX, nNextY = oNext:GetLocation()
    tData.nNextPoint = oWidget:AddAnimationPoint({x = nNextX, y = nNextY})
  end
  if tDisplayList[nIndex - 1] then
    local oPrevious = tDisplayList[nIndex - 1]
    local nPrevX, nPrevY = oPrevious:GetLocation()
    tData.nPrevPoint = oWidget:AddAnimationPoint({x = nPrevX, y = nPrevY})
  end
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  tData.nOriginPoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  tData.nSquishTop = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY1
  })
  tData.nSquishBottom = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY2,
    x2 = nX2,
    y2 = nY2
  })
  tData.nExitPoint = oWidget:AddAnimationPoint({
    x = nX1 - 25,
    y = nY1
  })
  tData.nEnterPoint = oWidget:AddAnimationPoint({x = nX1, y = nY1})
  local nSizeDiff = 0.75
  local oPrevIcon, oPrevIconBg, oNextIcon, oNextIconBg
  if tDisplayList[nIndex - 1] then
    local oPrev = tDisplayList[nIndex - 1]
    oPrevIcon = oPrev:GetChildren()[2]
    oPrevIconBg = oPrev:GetChildren()[1]
  end
  if tDisplayList[nIndex + 1] then
    local oNext = tDisplayList[nIndex + 1]
    oNextIcon = oNext:GetChildren()[2]
    oNextIconBg = oNext:GetChildren()[1]
  end
  _SetUpFlipPoints(oIcon, oPrevIcon, oNextIcon)
  _SetUpFlipPoints(oIconBg, oPrevIconBg, oNextIconBg)
  oWidget.FlipIconIn = _TriggerItemFlipIn
  oWidget.FlipIconOut = _TriggerItemFlipOut
  oBorder.CustomData.nFadeOutPoint = oBorder:AddAnimationPoint({TranslucencyLevel = 0})
  oBorder.CustomData.nFadeInPoint = oBorder:AddAnimationPoint({TranslucencyLevel = 255})
  local oNextBorder, oPrevBorder
  if tDisplayList[nIndex - 1] then
    oPrevBorder = tDisplayList[nIndex - 1]:GetChildren()[3]
  end
  if tDisplayList[nIndex + 1] then
    oNextBorder = tDisplayList[nIndex + 1]:GetChildren()[3]
  end
  _SetUpScaledPoints(oBorder, oPrevBorder, oNextBorder)
  oOrbit.CustomData.nFadeOutPoint = oOrbit:AddAnimationPoint({
    TranslucencyLevel = 0,
    nRotation = -90,
    nRotationDirection = -1
  })
  oOrbit.CustomData.nFadeInPoint = oOrbit:AddAnimationPoint({
    TranslucencyLevel = 255,
    nRotation = 0,
    nRotationDirection = 1
  })
  local oNextOrbit, oPrevOrbit
  if tDisplayList[nIndex - 1] then
    oPrevOrbit = tDisplayList[nIndex - 1]:GetChildren()[4]
  end
  if tDisplayList[nIndex + 1] then
    oNextOrbit = tDisplayList[nIndex + 1]:GetChildren()[4]
  end
  _SetUpScaledPoints(oOrbit, oPrevOrbit, oNextOrbit)
  oStatus.CustomData.nFadeInPoint = oStatus:AddAnimationPoint({TranslucencyLevel = 255})
  oStatus.CustomData.nFadeOutPoint = oStatus:AddAnimationPoint({TranslucencyLevel = 0})
  oStatus:SetVisible(false)
  oStatus:SetTranslucency(0)
  local oNextStatus, oPrevStatus
  if tDisplayList[nIndex - 1] then
    oPrevStatus = tDisplayList[nIndex - 1]:GetChildren()[5]
  end
  if tDisplayList[nIndex + 1] then
    oNextStatus = tDisplayList[nIndex + 1]:GetChildren()[5]
  end
  _SetUpScaledPoints(oStatus, oPrevStatus, oNextStatus)
  oWidget.SetItemTexture = _SetItemTexture
  oWidget.TriggerNext = _TriggerItemNext
  oWidget.TriggerPrev = _TriggerItemPrev
  oWidget.TriggerSquish = _TriggerSquish
  oWidget.SetupOpen = _SetupItemOpen
  oWidget.TriggerOpen = _TriggerItemOpenMain
  oWidget.TriggerClose = _TriggerItemClose
  oWidget.MoveTo = _MoveTo
  oWidget.SetStatus = _SetItemStatus
  oWidget:SetStatus(nil)
end

function _SetItemTexture(oWidget, sIcon)
  oWidget.CustomData.oIcon:SetTexture(sIcon)
end

function _SetItemStatus(oWidget, sStatus)
  local oStatus = oWidget.CustomData.oStatus
  if not sStatus then
    oStatus.CustomData.bInUse = false
    oStatus:SetTextureCoordinates(0.9921875, 0, 0.99609375, 1)
    return
  end
  if "fuel" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0, 0, 0.125, 1)
  elseif "[PDA.Support.denied.basic]" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0.375, 0, 0.5, 1)
  elseif "[PDA.Support.denied.medium]" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0.125, 0, 0.25, 1)
  elseif "[PDA.Support.denied.jammer]" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0.25, 0, 0.375, 1)
  elseif "[Generic.Attitudes.Hostile]" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0.625, 0, 0.75, 1)
  elseif "zero" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0.5, 0, 0.625, 1)
  elseif "disabled" == sStatus then
    oStatus.CustomData.bInUse = true
    oStatus:SetTextureCoordinates(0.75, 0, 0.875, 1)
  else
    oStatus.CustomData.bInUse = false
    oStatus:SetTextureCoordinates(0.9921875, 0, 0.99609375, 1)
  end
end

function _StartStatusPulse(oStatus)
  _StatusPulseToOpaque(oStatus)
end

function _HaltStatusPulse(oStatus, bVis)
  if bVis then
    oStatus:AnimateToPoint(oStatus.CustomData.nFadeInPoint, 0, true)
  else
    oStatus:AnimateToPoint(oStatus.CustomData.nFadeOutPoint, 0, true)
  end
end

function _StatusPulseToClear(oStatus)
  oStatus:AnimateToPoint(oStatus.CustomData.nFadeOutPoint, 0.75, true, _StatusPulseToOpaque)
end

function _StatusPulseToOpaque(oStatus)
  oStatus:AnimateToPoint(oStatus.CustomData.nFadeInPoint, 0.75, true, _StatusPulseToClear)
end

function _SetUpFlipPoints(oWidget, oPrev, oNext)
  local tData = oWidget.CustomData
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nWidth = nX2 - nX1
  local nHeight = nY2 - nY1
  tData.nScalePoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  tData.nOriginalWidth = nX2 - nX1
  tData.nOriginalHeight = nY2 - nY1
  if oPrev then
    local nPrevX1, nPrevY1, nPrevX2, nPrevY2 = oPrev:GetLocation()
    local nPrevWidth = nPrevX2 - nPrevX1
    local nPrevHeight = nPrevY2 - nPrevY1
    tData.nFlipPrevMidWidth = (nPrevWidth + nWidth) / 2
    tData.nFlipPrevMidHeight = 0
    tData.nFlipPrevEndWidth = nPrevWidth
    tData.nFlipPrevEndHeight = nPrevHeight
  end
  if oNext then
    local nNextX1, nNextY1, nNextX2, nNextY2 = oNext:GetLocation()
    local nNextWidth = nNextX2 - nNextX1
    local nNextHeight = nNextY2 - nNextY1
    tData.nFlipNextMidWidth = (nNextWidth + nWidth) / 2
    tData.nFlipNextMidHeight = 0
    tData.nFlipNextEndWidth = nNextWidth
    tData.nFlipNextEndHeight = nNextHeight
  end
  tData.nExitWidth = nWidth * 0.375
  tData.nExitHeight = nHeight
  oWidget.ScaleTo = _ScaleTo
end

function _SetUpScaledPoints(oWidget, oPrev, oNext)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nMidX = (nX2 + nX1) / 2
  local nMidY = (nY2 + nY1) / 2
  tData = oWidget.CustomData
  tData.nScalePoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  tData.nOriginalWidth = nX2 - nX1
  tData.nOriginalHeight = nY2 - nY1
  if oNext then
    local nNextX1, nNextY1, nNextX2, nNextY2 = oNext:GetLocation()
    tData.nNextWidth = nNextX2 - nNextX1
    tData.nNextHeight = nNextY2 - nNextY1
  end
  if oPrev then
    local nPrevX1, nPrevY1, nPrevX2, nPrevY2 = oPrev:GetLocation()
    tData.nPrevWidth = nPrevX2 - nPrevX1
    tData.nPrevHeight = nPrevY2 - nPrevY1
  end
  oWidget.ScaleTo = _ScaleTo
end

function _ScaleTo(oWidget, nWidth, nHeight, nTime, bImmediate, fCallback, tCallbackData)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nMidX = (nX1 + nX2) / 2
  local nMidY = (nY1 + nY2) / 2
  local tData = oWidget.CustomData
  local nNewX1 = nX1
  local nNewX2 = nX2
  if nWidth then
    nNewX1 = nMidX - nWidth / 2
    nNewX2 = nMidX + nWidth / 2
  end
  local nNewY1 = nY1
  local nNewY2 = nY2
  if nHeight then
    nNewY1 = nMidY - nHeight / 2
    nNewY2 = nMidY + nHeight / 2
  end
  if nTime and nTime <= 0 and bImmediate then
    oWidget:SetLocation(nNewX1, nNewY1, nNewX2, nNewY2)
    if "function" == type(fCallback) then
      local tRealCallbackData = tCallbackData
      if "table" ~= type(tRealCallbackData) then
        tRealCallbackData = {}
      end
      table.insert(tRealCallbackData, 1, oWidget)
      fCallback(unpack(tRealCallbackData))
    end
  else
    oWidget:SetAnimationPoint(tData.nScalePoint, {
      x = nNewX1,
      y = nNewY1,
      x2 = nNewX2,
      y2 = nNewY2
    })
    oWidget:AnimateToPoint(tData.nScalePoint, nTime, bImmediate, fCallback, tCallbackData)
  end
end

function _MoveTo(oWidget, nPoint, nTime, bImmediate, fCallback, tCallbackData)
  if nTime and nTime <= 0 and bImmediate then
    local tData = oWidget.AnimationPoints[nPoint]
    if not tData then
      return
    end
    oWidget:SetLocation(tData.nX1, tData.nY1, tData.nX2, tData.nY2)
    if "function" == type(fCallback) then
      local tRealCallbackData = tCallbackData
      if "table" ~= type(tRealCallbackData) then
        tRealCallbackData = {}
      end
      table.insert(tRealCallbackData, 1, oWidget)
      fCallback(unpack(tRealCallbackData))
    end
  else
    oWidget:AnimateToPoint(nPoint, nTime, bImmediate, fCallback, tCallbackData)
  end
end

function _TriggerItemNext(oWidget, nTime)
  local tData = oWidget.CustomData
  if not tData.nPrevPoint then
    return
  end
  oWidget:AnimateToPoint(tData.nPrevPoint, 0, true)
  oWidget:AnimateToPoint(tData.nOriginPoint, nTime, false)
  local oIconBg = tData.oIconBg
  local oIcon = tData.oIcon
  local nHalfTime = nTime / 2
  oIcon:ScaleTo(oIcon.CustomData.nFlipPrevEndWidth, oIcon.CustomData.nFlipPrevEndHeight, 0, true, oIcon.ScaleTo, {
    oIcon.CustomData.nFlipPrevMidWidth,
    oIcon.CustomData.nFlipPrevMidHeight,
    nHalfTime,
    true,
    oIcon.ScaleTo,
    {
      oIcon.CustomData.nOriginalWidth,
      oIcon.CustomData.nOriginalHeight,
      nHalfTime
    }
  })
  oIconBg:ScaleTo(oIconBg.CustomData.nFlipPrevEndWidth, oIconBg.CustomData.nFlipPrevEndHeight, 0, true, oIconBg.ScaleTo, {
    oIconBg.CustomData.nFlipPrevMidWidth,
    oIconBg.CustomData.nFlipPrevMidHeight,
    nHalfTime,
    true,
    oIconBg.ScaleTo,
    {
      oIconBg.CustomData.nOriginalWidth,
      oIconBg.CustomData.nOriginalHeight,
      nHalfTime
    }
  })
  local oBorder = tData.oBorder
  oBorder:ScaleTo(oBorder.CustomData.nPrevWidth, oBorder.CustomData.nPrevHeight, 0, true, oBorder.ScaleTo, {
    oBorder.CustomData.nOriginalWidth,
    oBorder.CustomData.nOriginalHeight,
    nTime,
    true
  })
  local oOrbit = tData.oOrbit
  oOrbit:ScaleTo(oOrbit.CustomData.nPrevWidth, oOrbit.CustomData.nPrevHeight, 0, true, oOrbit.ScaleTo, {
    oOrbit.CustomData.nOriginalWidth,
    oOrbit.CustomData.nOriginalHeight,
    nTime,
    true
  })
  local oStatus = tData.oStatus
  oStatus:ScaleTo(oStatus.CustomData.nPrevWidth, oStatus.CustomData.nPrevHeight, 0, true, oStatus.ScaleTo, {
    oStatus.CustomData.nOriginalWidth,
    oStatus.CustomData.nOriginalHeight,
    nTime,
    true
  })
end

function _TriggerItemPrev(oWidget, nTime)
  local tData = oWidget.CustomData
  if not tData.nNextPoint then
    return
  end
  oWidget:AnimateToPoint(tData.nNextPoint, 0, true)
  oWidget:AnimateToPoint(tData.nOriginPoint, nTime, false)
  local oIconBg = tData.oIconBg
  local oIcon = tData.oIcon
  local nHalfTime = nTime / 2
  oIcon:ScaleTo(oIcon.CustomData.nFlipNextEndWidth, oIcon.CustomData.nFlipNextEndHeight, 0, true, oIcon.ScaleTo, {
    oIcon.CustomData.nFlipNextMidWidth,
    oIcon.CustomData.nFlipNextMidHeight,
    nHalfTime,
    true,
    oIcon.ScaleTo,
    {
      oIcon.CustomData.nOriginalWidth,
      oIcon.CustomData.nOriginalHeight,
      nHalfTime
    }
  })
  oIconBg:ScaleTo(oIconBg.CustomData.nFlipNextEndWidth, oIconBg.CustomData.nFlipNextEndHeight, 0, true, oIconBg.ScaleTo, {
    oIconBg.CustomData.nFlipNextMidWidth,
    oIconBg.CustomData.nFlipNextMidHeight,
    nHalfTime,
    true,
    oIconBg.ScaleTo,
    {
      oIconBg.CustomData.nOriginalWidth,
      oIconBg.CustomData.nOriginalHeight,
      nHalfTime
    }
  })
  local oBorder = tData.oBorder
  oBorder:ScaleTo(oBorder.CustomData.nNextWidth, oBorder.CustomData.nNextHeight, 0, true, oBorder.ScaleTo, {
    oBorder.CustomData.nOriginalWidth,
    oBorder.CustomData.nOriginalHeight,
    nTime,
    true
  })
  local oOrbit = tData.oOrbit
  oOrbit:ScaleTo(oOrbit.CustomData.nNextWidth, oOrbit.CustomData.nNextHeight, 0, true, oOrbit.ScaleTo, {
    oOrbit.CustomData.nOriginalWidth,
    oOrbit.CustomData.nOriginalHeight,
    nTime,
    true
  })
  local oStatus = tData.oStatus
  oStatus:ScaleTo(oStatus.CustomData.nNextWidth, oStatus.CustomData.nNextHeight, 0, true, oStatus.ScaleTo, {
    oStatus.CustomData.nOriginalWidth,
    oStatus.CustomData.nOriginalHeight,
    nTime,
    true
  })
end

function _SetupItemNext(oWidget)
  local tData = oWidget.CustomData
  oWidget:MoveTo(tData.nPrevPoint, 0, true)
  local oIconBg = oWidget.CustomData.oIconBg
  local oIcon = oWidget.CustomData.oIcon
  oIcon:ScaleTo(oIcon.CustomData.nFlipPrevEndWidth, oIcon.CustomData.nFlipPrevEndHeight, 0, true)
  oIconBg:ScaleTo(oIconBg.CustomData.nFlipPrevEndWidth, oIconBg.CustomData.nFlipPrevEndHeight, 0, true)
  local oBorder = tData.oBorder
  oBorder:ScaleTo(oBorder.CustomData.nPrevWidth, oBorder.CustomData.nPrevHeight, 0, true)
  local oOrbit = tData.oOrbit
  oOrbit:ScaleTo(oOrbit.CustomData.nPrevWidth, oOrbit.CustomData.nPrevHeight, 0, true)
  local oStatus = tData.oStatus
  oStatus:ScaleTo(oStatus.CustomData.nPrevWidth, oStatus.CustomData.nPrevHeight, 0, true)
end

function _SetupItemPrev(oWidget)
  local tData = oWidget.CustomData
  oWidget:MoveTo(tData.nNextPoint, 0, true)
  local oIconBg = tData.oIconBg
  local oIcon = tData.oIcon
  oIcon:ScaleTo(oIcon.CustomData.nFlipNextEndWidth, oIcon.CustomData.nFlipNextEndHeight, 0, true)
  oIconBg:ScaleTo(oIconBg.CustomData.nFlipNextEndWidth, oIconBg.CustomData.nFlipNextEndHeight, 0, true)
  local oBorder = tData.oBorder
  oBorder:ScaleTo(oBorder.CustomData.nNextWidth, oBorder.CustomData.nNextHeight, 0, true)
  local oOrbit = tData.oOrbit
  oOrbit:ScaleTo(oOrbit.CustomData.nNextWidth, oOrbit.CustomData.nNextHeight, 0, true)
  local oStatus = tData.oStatus
  oStatus:ScaleTo(oStatus.CustomData.nNextWidth, oStatus.CustomData.nNextHeight, 0, true)
end

function _TriggerSquish(oItem, nTime, bEntering, bTop)
  local oIcon = oItem.CustomData.oIcon
  local oIconBg = oItem.CustomData.oIconBg
  if bEntering then
    oIcon:ScaleTo(oIcon.CustomData.nExitWidth, nil, 0, true, oIcon.ScaleTo, {
      oIcon.CustomData.nOriginalWidth,
      nil,
      nTime,
      true
    })
    oIconBg:ScaleTo(oIconBg.CustomData.nExitWidth, nil, 0, true, oIconBg.ScaleTo, {
      oIconBg.CustomData.nOriginalWidth,
      nil,
      nTime,
      true
    })
    oItem:AnimateToPoint(oItem.CustomData.nOriginPoint, nTime, true)
    oItem:SetVisible(true)
  else
    if bTop then
      oItem:AnimateToPoint(oItem.CustomData.nSquishTop, nTime, true)
    else
      oItem:AnimateToPoint(oItem.CustomData.nSquishBottom, nTime, true)
    end
    oIcon:ScaleTo(oIcon.CustomData.nOriginalWidth, nil, 0, true, oIcon.ScaleTo, {
      oIcon.CustomData.nExitWidth,
      nil,
      nTime,
      true
    })
    oIconBg:ScaleTo(oIconBg.CustomData.nOriginalWidth, nil, 0, true, oIconBg.ScaleTo, {
      oIconBg.CustomData.nExitWidth,
      nil,
      nTime,
      true
    })
  end
end

function _SetupSquish(oItem, bEntering, bTop)
  local oIcon = oItem.CustomData.oIcon
  local oIconBg = oItem.CustomData.oIconBg
  if bEntering then
    oItem:SetVisible(false)
    if bTop then
      oItem:AnimateToPoint(oItem.CustomData.nSquishTop, 0, true)
    else
      oItem:AnimateToPoint(oItem.CustomData.nSquishBottom, 0, true)
    end
    oIcon:ScaleTo(oIcon.CustomData.nExitWidth, nil, 0, true)
    oIconBg:ScaleTo(oIconBg.CustomData.nExitWidth, nil, 0, true)
  else
    oItem:AnimateToPoint(oItem.CustomData.nOriginPoint, 0, true)
    oIcon:ScaleTo(oIcon.CustomData.nOriginalWidth, nil, 0, true)
    oIconBg:ScaleTo(oIconBg.CustomData.nOriginalWidth, nil, 0, true)
  end
end

function _SetupItemOpen(oItem)
  local oIcon = oItem.CustomData.oIcon
  local oIconBg = oItem.CustomData.oIconBg
  local oBorder = oItem.CustomData.oBorder
  local oOrbit = oItem.CustomData.oOrbit
  oItem:MoveTo(oItem.CustomData.nExitPoint, 0, true)
  oIcon:ScaleTo(0, nil, 0, true)
  oIcon:SetVisible(false)
  oIconBg:ScaleTo(0, nil, 0, true)
  oIconBg:SetVisible(false)
  oBorder:SetTranslucency(0)
  oBorder:AnimateToPoint(oBorder.CustomData.nFadeOutPoint, 0, true)
  oOrbit:SetTranslucency(0)
  oOrbit:AnimateToPoint(oOrbit.CustomData.nFadeOutPoint, 0, true)
end

function _TriggerItemOpenMain(oItem, nTime)
  local oIcon = oItem.CustomData.oIcon
  local oIconBg = oItem.CustomData.oIconBg
  local oBorder = oItem.CustomData.oBorder
  local oStatus = oItem.CustomData.oStatus
  oItem:AnimateToPoint(oItem.CustomData.nEnterPoint, nTime, false)
  oBorder:AnimateToPoint(oBorder.CustomData.nFadeInPoint, nTime, false)
  oIcon:ScaleTo(0, nil, 0, true, oIcon.ScaleTo, {
    oIcon.CustomData.nOriginalWidth,
    nil,
    nTime,
    true
  })
  oIcon:SetVisible(true)
  oIconBg:ScaleTo(0, nil, 0, true, oIconBg.ScaleTo, {
    oIconBg.CustomData.nOriginalWidth,
    nil,
    nTime,
    true
  })
  oIconBg:SetVisible(true)
  if oStatus.CustomData.bInUse then
    oStatus:SetVisible(true)
    oStatus:SetTranslucency(0)
    oStatus:AnimateToPoint(oStatus.CustomData.nFadeInPoint, nTime, true)
  end
end

function _TriggerItemClose(oItem, nTime)
  local oIcon = oItem.CustomData.oIcon
  local oIconBg = oItem.CustomData.oIconBg
  local oBorder = oItem.CustomData.oBorder
  local oOrbit = oItem.CustomData.oOrbit
  local oStatus = oItem.CustomData.oStatus
  oItem:AnimateToPoint(oItem.CustomData.nExitPoint, nTime, false)
  oBorder:AnimateToPoint(oBorder.CustomData.nFadeOutPoint, nTime, false)
  oOrbit:AnimateToPoint(oOrbit.CustomData.nFadeOutPoint, nTime, false)
  oIcon:ScaleTo(oIcon.CustomData.nOriginalWidth, nil, 0, true, oIcon.ScaleTo, {
    0,
    nil,
    nTime,
    true
  })
  oIconBg:ScaleTo(oIconBg.CustomData.nOriginalWidth, nil, 0, true, oIconBg.ScaleTo, {
    0,
    nil,
    nTime,
    true
  })
  oStatus:SetTranslucency(255)
  oStatus:AnimateToPoint(oStatus.CustomData.nFadeOutPoint, nTime, true, oStatus.SetVisible, {false})
end

function _InitializeBullets(oWidget)
  oWidget.CustomData.nFadeInPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget.CustomData.nFadeOutPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  local tBullets = oWidget:GetChildren()
  local i = 1
  while tBullets[i] do
    tBullets[i].CustomData.nTargetPoint = tBullets[i]:AddAnimationPoint({})
    tBullets[i].CustomData.nOriginalRotation = tBullets[i]:GetRotation()
    i = i + 1
  end
  oWidget.AnimateBullet = _AnimateBullet
end

function _AnimateBullet(oWidget, nNumber, nTime, nFromRotationOffset, nToRotationOffset, nDirection, bHide)
  local oBullet = oWidget:GetChildren()[nNumber]
  if not oBullet then
    return
  end
  local nOriginalRotation = oBullet.CustomData.nOriginalRotation
  oBullet:SetRotation(nOriginalRotation + nFromRotationOffset)
  oBullet:SetAnimationPoint(oBullet.CustomData.nTargetPoint, {
    nRotation = nOriginalRotation + nToRotationOffset,
    nRotationDirection = nDirection
  })
  oBullet:AnimateToPoint(oBullet.CustomData.nTargetPoint, nTime, true, _ResetBullet, {bHide})
end

function _ResetBullet(oBullet, bHide)
  oBullet:SetRotation(oBullet.CustomData.nOriginalRotation)
  if bHide then
    oBullet:SetVisible(false)
  end
end

function _CreateFlyingIcon(oTemplateWidget)
  local oNewIcon = MrxGuiBase.ImageWidget:new()
  oNewIcon:SetVisible(false)
  oNewIcon.BasicData.bContainer = true
  oNewIcon:SetLocation(oTemplateWidget:GetLocation())
  oNewIcon:SetOwner(oTemplateWidget:GetOwner())
  local oIconBg = _CopyImageParameters(oTemplateWidget.CustomData.oIconBg)
  local oIcon = _CopyImageParameters(oTemplateWidget.CustomData.oIcon)
  local oBorder = _CopyImageParameters(oTemplateWidget.CustomData.oBorder)
  local oHighlight = _CopyImageParameters(oTemplateWidget.CustomData.oBorder)
  local oOrbit = _CopyImageParameters(oTemplateWidget.CustomData.oOrbit)
  local nU1, nV1, nU2, nV2 = oHighlight:GetTextureCoordinates()
  local nUOffset = 0.80859375
  local nVOffset = 0.24804688
  oHighlight:SetTextureCoordinates(nU1 + nUOffset, nV1 + nVOffset, nU2 + nUOffset, nV2 + nVOffset)
  oNewIcon:AddChild(oIconBg)
  oNewIcon:AddChild(oIcon)
  oNewIcon:AddChild(oBorder)
  oNewIcon:AddChild(oHighlight)
  oNewIcon:AddChild(oOrbit)
  oIconBg.ParentWidget = oNewIcon
  oIcon.ParentWidget = oNewIcon
  oBorder.ParentWidget = oNewIcon
  oHighlight.ParentWidget = oNewIcon
  oOrbit.ParentWidget = oNewIcon
  oNewIcon.CustomData.oIconBg = oIconBg
  oNewIcon.CustomData.oIcon = oIcon
  oNewIcon.CustomData.oBorder = oBorder
  oNewIcon.CustomData.oHighlight = oHighlight
  oNewIcon.CustomData.oOrbit = oOrbit
  oIconBg.ScaleTo = _ScaleTo
  oIconBg.CustomData.nScalePoint = oIconBg:AddAnimationPoint({})
  local nIconBgX1, nIconBgY1, nIconBgX2, nIconBgY2 = oIconBg:GetLocation()
  oIconBg.CustomData.nOriginalHeight = nIconBgY2 - nIconBgY1
  oIcon.ScaleTo = _ScaleTo
  oIcon.CustomData.nScalePoint = oIcon:AddAnimationPoint({})
  local nIconX1, nIconY1, nIconX2, nIconY2 = oIcon:GetLocation()
  oIcon.CustomData.nOriginalHeight = nIconY2 - nIconY1
  oHighlight.CustomData.oFadePoint = oHighlight:AddAnimationPoint({TranslucencyLevel = 0})
  oOrbit.CustomData.oNullPoint = oOrbit:AddAnimationPoint({})
  oOrbit.CustomData.oFadePoint = oOrbit:AddAnimationPoint({
    nRotation = 1,
    nRotationDirection = -1,
    TranslucencyLevel = 0
  })
  local oAmmoCounterImage = MrxGuiBase.GetWidgetByNameAndOwner("Ammo Counter Weapon Icon", oTemplateWidget:GetOwner())
  if oAmmoCounterImage then
    local nTargetIconX, nTargetIconY = oAmmoCounterImage:GetLocation()
    local nX, nY = oNewIcon:GetLocation()
    local nIconX, nIconY = oIcon:GetLocation()
    local nOffX = nX - nIconX
    local nOffY = nY - nIconY
    oNewIcon.CustomData.nTargetPoint = oNewIcon:AddAnimationPoint({
      x = nTargetIconX + nOffX,
      y = nTargetIconY + nOffY
    })
  else
    oNewIcon.CustomData.nTargetPoint = oNewIcon:AddAnimationPoint({TranslucencyLevel = 0})
  end
  oNewIcon.CustomData.nNullPoint = oNewIcon:AddAnimationPoint({})
  MrxGuiBase.AddWidgetWithChildren(oNewIcon)
  oHighlight:AnimateToPoint(oHighlight.CustomData.oFadePoint, _knFrame * 10, true, oHighlight.SetVisible, {false})
  oOrbit:AnimateToPoint(oOrbit.CustomData.oNullPoint, _knFrame * 5, true)
  oOrbit:AnimateToPoint(oOrbit.CustomData.oFadePoint, _knFrame * 20, false, oOrbit.SetVisible, {false})
  oNewIcon:AnimateToPoint(oNewIcon.CustomData.nNullPoint, _knFrame * 25, true, _StartIconTranslation)
  return oNewIcon
end

function _CopyImageParameters(oTemplateWidget)
  local oDestWidget = MrxGuiBase.ImageWidget:new()
  oDestWidget:SetOwner(oTemplateWidget:GetOwner())
  oDestWidget:SetLocation(oTemplateWidget:GetLocation())
  oDestWidget:SetTexture(oTemplateWidget:GetTexture())
  oDestWidget:SetTextureCoordinates(oTemplateWidget:GetTextureCoordinates())
  oDestWidget:SetAnchoring("left", "top")
  return oDestWidget
end

function _CompleteIconTranslation(oWidget)
  MrxGuiBase.RemoveWidgetWithChildren(oWidget)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:delete()
  end
  oWidget:delete()
end

function _StartIconTranslation(oWidget)
  local oGunAmmoCounter = MrxGuiBase.GetWidgetByNameAndOwner("Current Gun", oWidget:GetOwner())
  if oGunAmmoCounter then
    oGunAmmoCounter:TriggerAnimation(oWidget.CustomData.oIcon:GetTexture())
  end
  oWidget:AnimateToPoint(oWidget.CustomData.nTargetPoint, _knFrame * 10, true, _CompleteIconTranslation)
  local oIcon = oWidget.CustomData.oIcon
  local oIconBg = oWidget.CustomData.oIconBg
  oIcon:ScaleTo(nil, 0, _knFrame * 5, true, oIcon.ScaleTo, {
    nil,
    oIcon.CustomData.nOriginalHeight,
    _knFrame * 5,
    true
  })
  oIconBg:ScaleTo(nil, 0, _knFrame * 5, true, oIconBg.ScaleTo, {
    nil,
    oIconBg.CustomData.nOriginalHeight,
    _knFrame * 5,
    true
  })
end

function ValidateParameter(Parameter, sType, DefaultValue)
  if type(Parameter) == sType then
    return Parameter
  else
    return DefaultValue
  end
end

function Min(nA, nB)
  if nB < nA then
    return nB
  end
  return nA
end

function HandleInputEvent(oSupportMenu, tEvent)
  if not oSupportMenu.CustomData.bEnabled then
    return
  end
  if oSupportMenu.CustomData.bAddItemInProgress then
    if MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_ALT2_1 == tEvent.ButtonPress then
      oSupportMenu.CustomData.bSnapAddAnimation = true
      Event.Post("Support Menu Open", {
        uPlayer = oSupportMenu:GetOwner()
      })
    end
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_U == tEvent.ButtonPress then
    oSupportMenu:_ScrollDown()
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_D == tEvent.ButtonPress then
    oSupportMenu:_ScrollUp()
  elseif MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_ALT2_1 == tEvent.ButtonPress then
    oSupportMenu:Trigger()
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_L == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_PAD1_R == tEvent.ButtonPress then
  elseif tEvent.ButtonPress then
    oSupportMenu:Close()
  end
end

function WrapIndex(nIndex, nMaxIndex)
  if nMaxIndex < 1 then
    return nIndex
  end
  if 1 <= nIndex and nIndex <= nMaxIndex then
    return nIndex
  end
  if nIndex < 1 then
    while nIndex < 1 do
      nIndex = nIndex + nMaxIndex
    end
    return nIndex
  end
  if nMaxIndex < nIndex then
    while nMaxIndex < nIndex do
      nIndex = nIndex - nMaxIndex
    end
    return nIndex
  end
  return nIndex
end

function _AnimationCompleteCallback(oUnused, oWidget)
  oWidget.CustomData.bSuspendInput = false
  if oWidget.CustomData.bEnabled then
    UpdateDisplayText(oWidget)
  end
  if #oWidget.CustomData.tPendingItemQueue > 0 then
    _AddAnimationComplete(nil, oWidget)
  elseif oWidget.CustomData.bCloseOnComplete then
    oWidget.CustomData.bCloseOnComplete = false
    oWidget.CustomData.nBufferedInput = 0
    oWidget.CustomData.bConfirmEntered = nil
    oWidget:Close()
  elseif math.abs(oWidget.CustomData.nBufferedInput) > 0.5 then
    if 0 < oWidget.CustomData.nBufferedInput then
      oWidget.CustomData.nBufferedInput = oWidget.CustomData.nBufferedInput - 1
      oWidget:_ScrollUp()
    elseif 0 > oWidget.CustomData.nBufferedInput then
      oWidget.CustomData.nBufferedInput = oWidget.CustomData.nBufferedInput + 1
      oWidget:_ScrollDown()
    end
  elseif oWidget.CustomData.bConfirmEntered then
    oWidget:Trigger()
    oWidget.CustomData.bConfirmEntered = nil
  else
    local tDisplay = oWidget.CustomData.tDisplayList
    for n, oItem in ipairs(tDisplay) do
      local oStatus = oItem.CustomData.oStatus
      if oStatus.CustomData.bInUse then
        _StartStatusPulse(oStatus)
      else
        _HaltStatusPulse(oStatus)
      end
    end
    _HaltStatusPulse(tDisplay[1].CustomData.oStatus)
    _HaltStatusPulse(tDisplay[5].CustomData.oStatus)
  end
end

function _FrameAnimationCompleteCallback(oWidget, oParent)
  oParent.CustomData.oDescripters:SetVisible(true)
  _AnimationCompleteCallback(oWidget, oParent)
end

function _HandleGameStateChangeEvent(oWidget, sStateName, sStateAction)
  if sStateName and "SupportMenu" == sStateName and sStateAction then
    if "Enter" == sStateAction then
      oWidget:Open()
    end
    if "Exit" == sStateAction then
      oWidget:Close()
    end
  end
end

function _RemoveOnAnimationComplete(oWidget, oAmmoIcon)
  if oAmmoIcon then
    oAmmoIcon:SetTexture(oWidget:GetTexture())
    oAmmoIcon:SetVisible(true)
  end
  MrxGuiBase.RemoveWidget(oWidget)
  oWidget:delete()
end

function _RemoveAddAnimationComplete(oWidget)
  Event.Create(Event.TimerRelative, {0.5}, _DelayedSupportMenuCloseCallback, {
    oWidget.CustomData.oParentSupportMenu,
    oWidget.CustomData.fCallback,
    oWidget.CustomData.tCallbackData
  })
  MrxGuiBase.RemoveWidget(oWidget)
  _AddAnimationComplete(nil, oWidget.CustomData.oParentSupportMenu)
end

function _DelayedSupportMenuCloseCallback(oSupportMenu, fFunction, tCallbackData)
  oSupportMenu:Close()
  if "function" == type(fFunction) then
    local tData = tCallbackData
    if "table" ~= type(tCallbackData) then
      tData = {}
    end
    fFunction(unpack(tData))
  end
end

function _AddAnimationComplete(oUnused, oSupportMenu, fCallback, tCallbackData)
  if fCallback then
    fCallback(unpack(tCallbackData or {}))
  end
  oSupportMenu.CustomData.bAddItemInProgress = false
  if #oSupportMenu.CustomData.tPendingItemQueue <= 0 then
    return
  end
  oSupportMenu:AddItem(oSupportMenu.CustomData.tPendingItemQueue[1])
  table.remove(oSupportMenu.CustomData.tPendingItemQueue, 1)
end

function _PerformAddAnimation(oWidget, fCallback, tCallbackData)
  if not oWidget.CustomData.bEnabled then
    return
  end
  if oWidget.CustomData.nNumberOfItems < 1 then
    return
  end
  oWidget.CustomData.bSuspendInput = true
  oWidget.CustomData.bAddItemInProgress = true
  oWidget.CustomData.bSnapAddAnimation = false
  oWidget.CustomData.fAddCallback = fCallback
  oWidget.CustomData.tAddCallbackData = tCallbackData
  oWidget:_SetDisplayInformation()
  local tDisplay = oWidget.CustomData.tDisplayList
  tDisplay[2]:SetVisible(true)
  tDisplay[3]:SetVisible(false)
  tDisplay[4]:SetVisible(true)
  tDisplay[5]:SetVisible(true)
  tDisplay[2]:SetTranslucency(255)
  tDisplay[3]:SetTranslucency(255)
  tDisplay[4]:SetTranslucency(255)
  tDisplay[5]:SetTranslucency(255)
  _SetupItemNext(tDisplay[4])
  _SetupSquish(tDisplay[5], false, false)
  oWidget.CustomData.oDescripters:SetVisible(false)
  local oFrame = oWidget.CustomData.oFrame
  oFrame:AnimateToPoint(oFrame.CustomData.nClosePoint, _knFrame * 5, true, oFrame.SetVisible, {false})
  local oArrow = oWidget.CustomData.oDownArrow
  oArrow:SetVisible(true)
  oArrow:AnimateToPoint(oArrow.CustomData.nFadeInPoint, 0, true)
  oArrow:AnimateToPoint(oArrow.CustomData.nFadeOutPoint, _knFrame * 5, false)
  oWidget.CustomData.nTime = 0
  oWidget:SetEventHandler("GuiUpdate", _HandleUpdateForAdd)
  oWidget.CustomData.oClockIcon:SetVisible(false)
  Sound.CueSound(0, "ui_HUD_Support_Scroll")
end

function _HandleUpdateForAdd(oWidget, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local nFrame = _knFrame
  local nTimeStep = _knFrame
  if not oWidget.CustomData.nTime then
    oWidget.CustomData.nTime = 0
  end
  local nPrevTime = oWidget.CustomData.nTime
  local nNewTime = nPrevTime + nDeltaTime
  oWidget.CustomData.nTime = nNewTime
  local bSnap = false
  if oWidget.CustomData.bSnapAddAnimation then
    nNewTime = nNewTime + nFrame * 400
    nTimeStep = 0
    oWidget.CustomData.bSnapAddAnimation = false
    bSnap = true
  end
  local tDisplay = oWidget.CustomData.tDisplayList
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 4) then
    tDisplay[4]:TriggerNext(nTimeStep * 10)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 5) then
    tDisplay[5]:TriggerSquish(nTimeStep * 10, false, false)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 15) or bSnap then
    local oDisplayWidget = tDisplay[3]
    if _PassedPoint(nPrevTime, nNewTime, nFrame * 15) then
      local nDisplayX, nDisplayY = oDisplayWidget:GetLocation()
      oDisplayWidget:SetLocation(nDisplayX + 200, nDisplayY)
      oDisplayWidget:SetVisible(true)
    end
    oDisplayWidget:AnimateToPoint(oDisplayWidget.CustomData.nAddPoint, nTimeStep * 15, true)
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 30) or bSnap then
    local oFrame = oWidget.CustomData.oFrame
    oFrame:SetVisible(true)
    oFrame:AnimateToPoint(oFrame.CustomData.nOriginPoint, nTimeStep * 5, true, _FrameAnimationCompleteCallback, {oWidget})
    oWidget.CustomData.bSuspendInput = false
    oWidget.CustomData.bAddItemInProgress = false
  end
  if _PassedPoint(nPrevTime, nNewTime, nFrame * 150) or bSnap then
    oWidget.CustomData.nTime = 0
    oWidget:SetEventHandler("GuiUpdate", HandleUpdateForIdle)
    oWidget.CustomData.bAddItemInProgress = false
    oWidget.CustomData.bSuspendInput = false
    if not bSnap and oWidget.CustomData.bAnimatingAdd then
      oWidget:Close()
    end
    oWidget.CustomData.bAnimatingAdd = false
    if "function" == type(oWidget.CustomData.fAddCallback) then
      local tCallbackData = oWidget.CustomData.tAddCallbackData
      if "table" ~= type(tCallbackData) then
        tCallbackData = {}
      end
      oWidget.CustomData.fAddCallback(unpack(tCallbackData))
    end
  end
end

function InitAddWidget(nX, nY, nSize, uOwner)
  local oAdd = MrxGuiBase.Widget:new()
  oAdd:SetLocation(nX, nY, nX + 200, nY + nSize)
  oAdd:SetOwner(uOwner)
  local oIconBg = MrxGuiBase.ImageWidget:new()
  oIconBg:SetLocation(nX, nY, nX + nSize, nY + nSize)
  oIconBg:SetTexture("global_gui_hud02")
  oIconBg:SetTextureCoordinates(0.195313, 0.462891, 0.378906, 0.646484)
  oAdd:AddChild(oIconBg)
  local nIconCenterX = nX + nSize * 0.5
  oIconBg.CustomData.nHidePoint = oIconBg:AddAnimationPoint({
    x = nIconCenterX,
    y = nY,
    x2 = nIconCenterX,
    y2 = nY + nSize
  })
  oIconBg.CustomData.nShowPoint = oIconBg:AddAnimationPoint({
    x = nX,
    y = nY,
    x2 = nX + nSize,
    y2 = nY + nSize
  })
  oIconBg:SetOwner(uOwner)
  local oIcon = MrxGuiBase.ImageWidget:new()
  oIcon:SetLocation(nX, nY, nX + nSize, nY + nSize)
  oIcon:SetOwner(uOwner)
  oIcon.CustomData.nHidePoint = oIcon:AddAnimationPoint({
    x = nIconCenterX,
    y = nY,
    x2 = nIconCenterX,
    y2 = nY + nSize
  })
  oIcon.CustomData.nShowPoint = oIcon:AddAnimationPoint({
    x = nX,
    y = nY,
    x2 = nX + nSize,
    y2 = nY + nSize
  })
  oAdd:AddChild(oIcon)
  local oIconFrame = MrxGuiBase.ImageWidget:new()
  oIconFrame:SetLocation(nX, nY, nX + nSize, nY + nSize)
  oIconFrame:SetTexture("global_gui_hud02")
  oIconFrame:SetTextureCoordinates(0, 0.458985, 0.1875, 0.646484)
  oIconFrame:SetOwner(uOwner)
  oIconFrame.CustomData.nHidePoint = oIconFrame:AddAnimationPoint({
    x = nIconCenterX,
    y = nY,
    x2 = nIconCenterX,
    y2 = nY + nSize
  })
  oIconFrame.CustomData.nShowPoint = oIconFrame:AddAnimationPoint({
    x = nX,
    y = nY,
    x2 = nX + nSize,
    y2 = nY + nSize
  })
  oAdd:AddChild(oIconFrame)
  local oText = MrxGuiBase.TextWidget:new()
  oText:SetLocation(nX + nSize + 4, nY + 10)
  oText:SetFont("english_18")
  oText.CustomData.nShowPoint = oText:AddAnimationPoint({TranslucencyLevel = 255})
  oText.CustomData.nHidePoint = oText:AddAnimationPoint({TranslucencyLevel = 0})
  oText:SetOwner(uOwner)
  oAdd:AddChild(oText)
  oAdd.CustomData.tQueue = {}
  oAdd.CustomData.oIcon = oIcon
  oAdd.CustomData.oText = oText
  oAdd.CustomData.oIconBg = oIconBg
  oAdd.CustomData.oIconFrame = oIconFrame
  oAdd.CustomData.bActive = false
  _GuiInternal.SetWidgetUseNewRescale(oAdd.BasicData.uId, true)
  oAdd.Show = ShowNewAddAnimation
  oAdd.Hide = HideNewAddAnimation
  oAdd.Remove = RemoveNewAddItem
  MrxGuiBase.AddWidgetWithChildren(oAdd)
  oAdd:Hide()
  return oAdd
end

function ShowNewAddAnimation(oAdd, sIcon, sText)
  table.insert(oAdd.CustomData.tQueue, {sIcon, sText})
  if not oAdd.CustomData.bActive then
    oAdd.CustomData.bActive = true
    oAdd:SetVisible(true)
    local oIconBg = oAdd.CustomData.oIconBg
    oIconBg:AnimateToPoint(oIconBg.CustomData.nHidePoint, 0.01, true, _NewAddProcessQueue, {oAdd, true})
  end
end

function HideNewAddAnimation(oAdd)
  oAdd:SetVisible(false)
  local oIconBg = oAdd.CustomData.oIconBg
  local oIcon = oAdd.CustomData.oIcon
  local oIconFrame = oAdd.CustomData.oIconFrame
  local oText = oAdd.CustomData.oText
  oIconBg:AnimateToPoint(oIconBg.CustomData.nHidePoint, 0, true)
  oText:AnimateToPoint(oText.CustomData.nHidePoint, 0, true)
  oIcon:AnimateToPoint(oIcon.CustomData.nHidePoint, 0, true)
  oIconFrame:AnimateToPoint(oIconFrame.CustomData.nHidePoint, 0, true)
  oAdd.CustomData.bActive = false
  oAdd.CustomData.tQueue = {}
end

function RemoveNewAddItem(oAdd, sText)
  local nDataFound
  for n, tData in pairs(oAdd.CustomData.tQueue) do
    if tData[2] == sText then
      nDataFound = n
    end
  end
  if nDataFound then
    table.remove(oAdd.CustomData.tQueue, nDataFound)
  end
end

function _NewAddProcessQueue(oUnused, oAdd, bInitial)
  if #oAdd.CustomData.tQueue < 1 then
    oAdd:Hide()
    return
  end
  local oIconBg = oAdd.CustomData.oIconBg
  local oIcon = oAdd.CustomData.oIcon
  local oIconFrame = oAdd.CustomData.oIconFrame
  local oText = oAdd.CustomData.oText
  local tNextData = oAdd.CustomData.tQueue[1]
  table.remove(oAdd.CustomData.tQueue, 1)
  oIcon:SetTexture(tNextData[1])
  oIcon:SetTextureCoordinates(0, 0, 1, 1)
  oText:SetText(tNextData[2])
  local nAnimTime = 0.15
  local nShowTime = 2
  if true == bInitial then
    oIcon:SetTextureCoordinates(1, 0, 0, 1)
    oIconBg:AnimateToPoint(oIconBg.CustomData.nShowPoint, nAnimTime, true, _NewAddStepInitial2, {
      oText,
      oIcon,
      oIconFrame,
      oAdd
    })
    oText:AnimateToPoint(oText.CustomData.nHidePoint, 0, true)
  else
    oIconBg:AnimateToPoint(oIconBg.CustomData.nShowPoint, nAnimTime, true, _NewAddStep2, {
      oText,
      oIcon,
      oIconFrame,
      oAdd
    })
    oText:AnimateToPoint(oText.CustomData.nShowPoint, nAnimTime, true)
  end
  oIcon:AnimateToPoint(oIcon.CustomData.nShowPoint, nAnimTime, true)
  oIconFrame:AnimateToPoint(oIconFrame.CustomData.nShowPoint, nAnimTime, true)
end

function _NewAddStep2(oIconBg, oText, oIcon, oIconFrame, oAdd)
  local nAnimTime = 0.15
  local nShowTime = 2
  oIcon:SetTextureCoordinates(0, 0, 1, 1)
  oIconBg:AnimateToPoint(oIconBg.CustomData.nShowPoint, nShowTime, true, _NewAddStep3, {
    oText,
    oIcon,
    oIconFrame,
    oAdd
  })
  oText:AnimateToPoint(oText.CustomData.nShowPoint, nShowTime, true)
  oIcon:AnimateToPoint(oIcon.CustomData.nShowPoint, nShowTime, true)
  oIconFrame:AnimateToPoint(oIconFrame.CustomData.nShowPoint, nShowTime, true)
end

function _NewAddStep3(oIconBg, oText, oIcon, oIconFrame, oAdd)
  local nAnimTime = 0.15
  local nShowTime = 2
  oIconBg:AnimateToPoint(oIconBg.CustomData.nHidePoint, nAnimTime, true, _NewAddProcessQueue, {oAdd})
  oText:AnimateToPoint(oText.CustomData.nHidePoint, nAnimTime, true)
  oIcon:AnimateToPoint(oIcon.CustomData.nHidePoint, nAnimTime, true)
  oIconFrame:AnimateToPoint(oIconFrame.CustomData.nHidePoint, nAnimTime, true)
end

function _NewAddStepInitial2(oIconBg, oText, oIcon, oIconFrame, oAdd)
  local nAnimTime = 0.15
  local nShowTime = 2
  oIconBg:AnimateToPoint(oIconBg.CustomData.nHidePoint, nAnimTime, true, _NewAddStepInitial3, {
    oText,
    oIcon,
    oIconFrame,
    oAdd
  })
  oText:AnimateToPoint(oText.CustomData.nHidePoint, nAnimTime, true)
  oIcon:AnimateToPoint(oIcon.CustomData.nHidePoint, nAnimTime, true)
  oIconFrame:AnimateToPoint(oIconFrame.CustomData.nHidePoint, nAnimTime, true)
end

function _NewAddStepInitial3(oIconBg, oText, oIcon, oIconFrame, oAdd)
  local nAnimTime = 0.15
  local nShowTime = 2
  oIcon:SetTextureCoordinates(0, 0, 1, 1)
  oIconBg:AnimateToPoint(oIconBg.CustomData.nShowPoint, nAnimTime, true, _NewAddStep2, {
    oText,
    oIcon,
    oIconFrame,
    oAdd
  })
  oText:AnimateToPoint(oText.CustomData.nShowPoint, nAnimTime, true)
  oIcon:AnimateToPoint(oIcon.CustomData.nShowPoint, nAnimTime, true)
  oIconFrame:AnimateToPoint(oIconFrame.CustomData.nShowPoint, nAnimTime, true)
end
