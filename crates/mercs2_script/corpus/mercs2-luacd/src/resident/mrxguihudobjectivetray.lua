import("MrxGui")
import("MrxGuiBase")
import("MrxGuiManager")

function SetSlotToWidget(oTray, nSlot, oWidget)
  if "number" ~= type(nSlot) then
    return false
  end
  if nSlot < 1 or 3 < nSlot then
    return false
  end
  if "table" ~= type(oWidget) then
    return false
  end
  oTray:ClearSlot(nSlot)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  nX2 = nX2 or nX1 + oTray.CustomData.nDefaultWidth
  nY2 = nY2 or nY1 + oTray.CustomData.nDefaultHeight
  if not nX2 or not nY2 then
    oWidget:SetLocation(nX1, nY1, nX2, nY2)
  end
  local nSlotX, nSlotY = oTray:GetChildren()[nSlot]:GetLocation()
  oTray:GetChildren()[nSlot]:SetLocation(nil, nSlotY, nil, nSlotY + oTray.CustomData.nDefaultHeight)
  oWidget:SetOwner(oTray:GetOwner())
  local oSlot = oTray:GetChildren()[nSlot]
  local nNewX1, nNewY1, nNewX2 = oSlot:GetLocation()
  local nWidgetWidth = nX2 - nX1
  oWidget:SetLocation(nNewX2 - nWidgetWidth, nNewY1)
  if oSlot:GetChildren()[1] then
    local oDeadWidget = oSlot:GetChildren()[1]
    oSlot:RemoveChild(oDeadWidget)
    MrxGui.RemoveWidget(oDeadWidget)
    MrxGuiManager.RemoveWidgetFromHud(oDeadWidget:GetOwner(), oDeadWidget)
    oDeadWidget:delete()
  end
  oWidget:SetOwner(oTray:GetOwner())
  oWidget:SetVisible(oTray:GetVisible())
  oSlot:AddChild(oWidget)
  MrxGuiManager.AddWidgetToHud(oWidget:GetOwner(), oWidget)
  MrxGui.AddWidgetWithChildren(oWidget)
  return true
end

function SetSlotToText(oTray, nSlot, sText)
  if "string" ~= type(sText) then
    return nil
  end
  local oSlot = oTray:GetChildren()[nSlot]
  local oSlotDisplay
  if oSlot then
    oSlotDisplay = oSlot:GetChildren()[1]
  else
    return nil
  end
  if oSlotDisplay and "text" == oSlotDisplay.BasicData.type then
    oSlotDisplay:SetText(sText)
    return oSlotDisplay
  end
  local oNewTextWidget = MrxGui.TextWidget:new()
  oNewTextWidget:SetFont("english_18")
  oNewTextWidget:SetScale(1)
  oNewTextWidget:SetText(sText)
  oNewTextWidget:SetLocation(0, 0, oTray.CustomData.nDefaultWidth, oTray.CustomData.nDefaultHeight)
  oNewTextWidget:SetLocation(0, 0, oTray.CustomData.nDefaultWidth, oNewTextWidget:GetHeight())
  oNewTextWidget:SetJustification("right")
  oNewTextWidget:SetAnchoring("left", "top")
  if oTray:SetSlotToWidget(nSlot, oNewTextWidget) then
    return oNewTextWidget
  end
  oNewTextWidget:delete()
  return nil
end

function SetSlotToImage(oTray, nSlot, sTexture, nTextureWidth, nTextureHeight)
  if "string" ~= type(sTexture) then
    return nil
  end
  nTextureWidth = nTextureWidth or oTray.CustomData.nDefaultWidth
  nTextureHeight = nTextureHeight or oTray.CustomData.nDefaultHeight
  local oSlot = oTray:GetChildren()[nSlot]
  local oSlotDisplay
  if oSlot then
    oSlotDisplay = oSlot:GetChildren()[1]
  else
    return nil
  end
  if oSlotDisplay and "image" == oSlotDisplay.BasicData.type then
    oSlotDisplay:SetTexture(sTexture)
    local nX, nY = oSlotDisplay:GetLocation()
    oSlotDisplay:SetLocation(nX, nY, nX + nTextureWidth, nY + nTextureHeight)
    return oSlotDisplay
  end
  local oNewImageWidget = MrxGui.ImageWidget:new()
  oNewImageWidget:SetTexture(sTexture)
  oNewImageWidget:SetLocation(0, 0, nTextureWidth, nTextureHeight)
  oNewImageWidget:SetAnchoring("left", "top")
  if oTray:SetSlotToWidget(nSlot, oNewImageWidget) then
    return oNewImageWidget
  end
  return nil
end

function ClearSlot(oTray, nSlot)
  local oSlot = oTray:GetChildren()[nSlot]
  if not oSlot or not oSlot:GetChildren()[1] then
    return
  end
  local nX1, nY1, nX2, nY2 = oSlot:GetLocation()
  if nY2 - nY1 > oTray.CustomData.nDefaultHeight then
    local nIndex = nSlot + 1
    local nYDifference = nY2 - nY1 - oTray.CustomData.nDefaultHeight
    while oTray:GetChildren()[nIndex] do
      local nSlotX, nSlotY = oTray:GetChildren()[nIndex]:GetLocation()
      oTray:GetChildren()[nIndex]:SetLocation(nil, nSlotY - nYDifference)
      nIndex = nIndex + 1
    end
    oSlot:SetLocation(nil, nY1, nil, nY1 + oTray.CustomData.nDefaultHeight)
  end
  MrxGui.RemoveWidgetWithChildren(oSlot)
  local oDeadWidget = oSlot:GetChildren()[1]
  oSlot:RemoveChild(oDeadWidget)
  MrxGuiManager.RemoveWidgetFromHud(oDeadWidget:GetOwner(), oDeadWidget)
  oDeadWidget:delete()
end

function IsSlotOccupied(oTray, nSlot)
  if not nSlot then
    return false
  end
  local oSlot = oTray:GetChildren()[nSlot]
  if oSlot and oSlot:GetChildren()[1] then
    return true
  end
  return false
end

function _HandleInitializationEvent(oWidget, tEvent)
  local nSlots = 3
  local nSpacing = 2
  local nDefaultHeight = 16
  oWidget.CustomData.nSpacing = 5
  oWidget.CustomData.nDefaultHeight = nDefaultHeight
  local nSlotSpace = oWidget.CustomData.nSpacing + oWidget.CustomData.nDefaultHeight
  local nIndex = 1
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  oWidget.CustomData.nDefaultWidth = nX2 - nX1
  while nSlots >= nIndex do
    local oNewSlot = MrxGui.Widget:new()
    oNewSlot:SetOwner(oWidget:GetOwner())
    oNewSlot:SetLocation(nX1, nY1 + (nIndex - 1) * nSlotSpace, nX2, nY1 + (nIndex - 1) * nSlotSpace + nDefaultHeight)
    oNewSlot:SetVisible(false)
    oNewSlot:SetAnchoring("left", "top")
    oWidget:AddChild(oNewSlot)
    nIndex = nIndex + 1
    MrxGuiManager.AddWidgetToHud(oWidget:GetOwner(), oNewSlot)
  end
  oWidget.SetSlotToWidget = SetSlotToWidget
  oWidget.SetSlotToText = SetSlotToText
  oWidget.SetSlotToImage = SetSlotToImage
  oWidget.ClearSlot = ClearSlot
  oWidget.IsSlotOccupied = IsSlotOccupied
end
