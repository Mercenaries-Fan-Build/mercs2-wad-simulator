import("WifMissionData")
import("MrxSupportData")
import("WifEquipmentData")
import("MrxPmc")
import("MrxUtil")
import("MrxFactionManager")
import("MrxRewardData")
tTypeToIcon = {
  Airstrike = "[airstrike] ",
  Supply = "[supply] ",
  Light = "[vehmlight] ",
  Heavy = "[vehmheavy] ",
  Civilian = "[vehcivilian] ",
  Boat = "[vehboat] ",
  Heli = "[vehheli] ",
  [WifEquipmentData.knTypeFuelTank] = "[fuelsilo] "
}
_tGlobalShopList = {}
_oVender = nil

function Init()
  Debug.Printf("Shop - Generating Global ShopList...")
  local tFactions = MrxFactionManager.GetFactionAbbrevs()
  for _, sFaction in pairs(tFactions) do
    _tGlobalShopList[sFaction] = {
      tPurchased = {
        nSupport = 0,
        tSupport = {},
        nEquipment = 0,
        tEquipment = {}
      }
    }
  end
end

function Open(oVender, fOnClose, tOnCloseData)
  _oVender = oVender
  local uPlayer = Player.GetLocalPlayer()
  Hud.Shop:Create({uPlayer = uPlayer})
  Hud.Shop:SetCallback({
    uPlayer = uPlayer,
    fCallback = _ShopSelection,
    tCallbackData = {}
  })
  Hud.Shop:SetCloseCallback({
    uPlayer = uPlayer,
    fCallback = function()
      _oVender = nil
      MrxUtil.CallWithOptionalArgs(fOnClose, tOnCloseData)
    end
  })
  local sFactionId = oVender:GetFaction()
  local bObscureShopInfo = oVender:HasCustomVehicleShop()
  local nPriceScale = _GetPriceScale(oVender)
  local tShopList = _GetShopList(oVender)
  local nItem = 1
  Debug.Printf("Shop - Populating shop...")
  local tSupportUnlocked = {}
  local tSupportLocked = {}
  local tEquipmentUnlocked = {}
  local tEquipmentLocked = {}
  for sId, _ in pairs(tShopList.tSupport) do
    local tSupport = MrxSupportData.tSupportData[sId]
    if tSupport == nil then
      Debug.Printf("Shop - No support data for " .. sId)
    else
      local bIsUnlocked
      if Net.IsClient() then
        bIsUnlocked = _tIndexedShopList[nItem]
      else
        bIsUnlocked = MrxSupportData.IsItemUnlocked(sId, sFactionId)
      end
      Debug.Printf("Shop - Adding Support " .. sId .. " to shop. Status - " .. tostring(bIsUnlocked))
      local sName = tSupport.sName
      local sDescription = tSupport.sDescription
      local sShopId = sId
      local sIcon = tTypeToIcon[tSupport.sType]
      if not bIsUnlocked and bObscureShopInfo then
        sName = "[Shop.LockedItem]"
        sDescription = "[Shop.LockedItem]"
        sShopId = "Locked"
        sIcon = nil
      end
      sIcon = sIcon or ""
      local bIsNew = MrxSupportData.IsItemNew(sId, sFactionId)
      if bIsNew then
        MrxSupportData.SetItemViewed(sId, sFactionId)
      end
      local tList
      if bIsUnlocked then
        tList = tSupportUnlocked
      else
        tList = tSupportLocked
      end
      table.insert(tList, {
        uPlayer = uPlayer,
        sName = sIcon .. sName,
        sId = sShopId,
        sDescription = sDescription,
        sTexture = tSupport.sIcon,
        nCashCost = tSupport.nCashCost * nPriceScale,
        nCurrentStock = MrxPmc.GetSupportQty(sId) or 0,
        nMaxStock = tSupport.nMaxStock,
        bFuelTank = false,
        bUnlocked = bIsUnlocked,
        bMarkAsNew = bIsNew,
        sRawName = sName
      })
      nItem = nItem + 1
    end
  end
  for sId, _ in pairs(tShopList.tEquipment) do
    local tEquipmentData = WifEquipmentData.GetEquipmentData(sId)
    if tEquipmentData == nil then
      Debug.Printf("Shop - No equipment data for " .. sId)
    else
      local bIsUnlocked
      if Net.IsClient() then
        bIsUnlocked = _tIndexedShopList[nItem]
      else
        bIsUnlocked = WifEquipmentData.IsItemUnlocked(sId, sFactionId)
      end
      Debug.Printf("Shop - Adding Equipment " .. sId .. " to shop. Status - " .. tostring(bIsUnlocked))
      local sName = tEquipmentData.sName
      local sDescription = tEquipmentData.sDescription
      local sShopId = sId
      local sIcon = tTypeToIcon[tEquipmentData.nType]
      if not bIsUnlocked and bObscureShopInfo then
        sName = "[Shop.LockedItem]"
        sDescription = "[Shop.LockedItem]"
        sShopId = "Locked"
        sIcon = nil
      end
      local bIsNew = WifEquipmentData.IsItemNew(sId, sFactionId)
      if bIsNew then
        WifEquipmentData.SetItemViewed(sId, sFactionId)
      end
      sIcon = sIcon or ""
      local nCurrentStock = 0
      if MrxPmc.HasEquipment(sId) then
        nCurrentStock = 1
      end
      local tList
      if bIsUnlocked then
        tList = tEquipmentUnlocked
      else
        tList = tEquipmentLocked
      end
      local bIsFuelTank = tEquipmentData.nType == WifEquipmentData.knTypeFuelTank
      local bIsGrapplingHook = tEquipmentData.nType == WifEquipmentData.knTypeGrapplingHook
      local bOkToInsert = bIsFuelTank or bIsGrapplingHook and nCurrentStock == 0
      if Net.IsClient() and bIsGrapplingHook then
        bOkToInsert = false
      end
      if bOkToInsert then
        table.insert(tList, {
          uPlayer = uPlayer,
          sName = sIcon .. sName,
          sId = sShopId,
          sDescription = sDescription,
          sTexture = tEquipmentData.sTexture,
          nCashCost = tEquipmentData.nCost * nPriceScale,
          nCurrentStock = nCurrentStock,
          nMaxStock = 1,
          bFuelTank = bIsFuelTank,
          bUnlocked = bIsUnlocked,
          bMarkAsNew = bIsNew,
          nFuelQuantity = tEquipmentData.nFuelCapacity,
          sRawName = sName
        })
        nItem = nItem + 1
      end
    end
  end
  
  local function sort(a, b)
    return a.nCashCost < b.nCashCost
  end
  
  table.sort(tSupportUnlocked, sort)
  table.sort(tSupportLocked, sort)
  table.sort(tEquipmentUnlocked, sort)
  table.sort(tEquipmentLocked, sort)
  for _, tItem in ipairs(tSupportUnlocked) do
    Hud.Shop:AddItemFull(tItem)
  end
  for _, tItem in ipairs(tEquipmentUnlocked) do
    Hud.Shop:AddItemFull(tItem)
  end
  for _, tItem in ipairs(tSupportLocked) do
    Hud.Shop:AddItemFull(tItem)
  end
  for _, tItem in ipairs(tEquipmentLocked) do
    Hud.Shop:AddItemFull(tItem)
  end
  Hud.Shop:Commence({uPlayer = uPlayer})
end

function _GetShopList(oVender, sFactionId)
  local tItems = {
    nSupport = 0,
    tSupport = {},
    nEquipment = 0,
    tEquipment = {}
  }
  Debug.Printf("Shop - Generating Shop List...")
  if oVender and not sFactionId then
    sFactionId = oVender:GetFaction()
  end
  local tSupport, tEquipment
  if sFactionId then
    tSupport, tEquipment = MrxRewardData.GetAllPotentialShopItems(sFactionId)
  end
  if tSupport then
    for _, sId in ipairs(tSupport) do
      if not tItems.tSupport[sId] then
        tItems.tSupport[sId] = true
        tItems.nSupport = tItems.nSupport + 1
      end
    end
  end
  if tEquipment then
    for _, sId in ipairs(tEquipment) do
      if not tItems.tEquipment[sId] then
        tItems.tEquipment[sId] = true
        tItems.nEquipment = tItems.nEquipment + 1
      end
    end
  end
  return tItems
end

function _GetPriceScale(oVender)
  if oVender:HasCustomVehicleShop() then
    return 1
  end
  local sFactionId = oVender:GetFaction()
  return MrxFactionManager.GetPriceScale(sFactionId, "Pmc")
end

function _ShopSelection(sId, nAmt)
  if sId == "Locked" then
    return false
  end
  local nPriceScale = _GetPriceScale(_oVender)
  local tSupport = MrxSupportData.tSupportData[sId]
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sId)
  if tSupport then
    local nCost = tSupport.nCashCost * nPriceScale * nAmt
    if nCost <= MrxPmc.GetCashQty() then
      Debug.Printf("Shop - Purchased " .. tostring(nAmt) .. " of Support " .. sId .. " for " .. tostring(nCost))
      MrxPmc.AddSupportQty(sId, nAmt, false, nCost)
      MrxPmc.AddCashQty(-nCost, nil, "[Generic.ShopItems]")
      _AddPurchasedSupportItem(sId)
      return true
    end
  elseif tEquipmentData then
    local nCost = tEquipmentData.nCost * nPriceScale * nAmt
    if nCost <= MrxPmc.GetCashQty() then
      Debug.Printf("Shop - Purchased " .. tostring(nAmt) .. " of Equipment " .. sId .. " for " .. tostring(nCost))
      MrxPmc.AddEquipment(sId)
      MrxPmc.AddCashQty(-nCost, nil, "[Generic.ShopItems]")
      _AddPurchasedEquipmentItem(sId)
      return true
    end
  end
  return false
end

function _AddPurchasedSupportItem(sId)
  local tFaction = _tGlobalShopList[_oVender:GetFaction()]
  if not tFaction then
    return
  end
  if tFaction.tPurchased.tSupport[sId] then
    return
  end
  tFaction.tPurchased.nSupport = tFaction.tPurchased.nSupport + 1
  tFaction.tPurchased.tSupport[sId] = true
end

function _AddPurchasedEquipmentItem(sId)
  local tFaction = _tGlobalShopList[_oVender:GetFaction()]
  if not tFaction then
    return
  end
  if tFaction.tPurchased.tEquipment[sId] then
    return
  end
  tFaction.tPurchased.nEquipment = tFaction.tPurchased.nEquipment + 1
  tFaction.tPurchased.tEquipment[sId] = true
end

function Close()
  local uPlayer = Player.GetLocalPlayer()
  Hud.Shop:Close({uPlayer = uPlayer})
end

function GetIndexedShopList(oVender)
  local tShopList = _GetShopList(oVender)
  local tIndexedList = {}
  local nItem = 1
  local sFactionId = oVender:GetFaction()
  for sId, _ in pairs(tShopList.tSupport) do
    tIndexedList[nItem] = MrxSupportData.IsItemUnlocked(sId, sFactionId)
    nItem = nItem + 1
  end
  for sId, _ in pairs(tShopList.tEquipment) do
    tIndexedList[nItem] = WifEquipmentData.IsItemUnlocked(sId, sFactionId)
    nItem = nItem + 1
  end
  return tIndexedList
end

function SetIndexedShopList(tIndexedList)
  _tIndexedShopList = tIndexedList
end

function GetTotalNumberOfItems(sFaction)
  local tFaction = _tGlobalShopList[sFaction]
  if not tFaction then
    return -1
  end
  if not tFaction.nTotalItems then
    Debug.Printf("Shop - Generate totals for Faction " .. sFaction .. "...")
    local tShopList = _GetShopList(nil, sFaction)
    tFaction.nTotalItems = tShopList.nSupport + tShopList.nEquipment
  end
  Debug.Printf("Shop - Faction " .. sFaction .. " has " .. tFaction.nTotalItems .. " items")
  return tFaction.nTotalItems
end

function GetNumberOfPurchasedItems(sFaction)
  local tFaction = _tGlobalShopList[sFaction]
  if not tFaction then
    return -1
  end
  return tFaction.tPurchased.nSupport + tFaction.tPurchased.nEquipment
end

function GetNumberOfUnlockedItems(sFaction)
  local tShopList = _GetShopList(nil, sFaction)
  local nUnlocked = 0
  for sId, _ in pairs(tShopList.tSupport) do
    if MrxSupportData.IsItemUnlocked(sId, sFaction) then
      nUnlocked = nUnlocked + 1
    end
  end
  for sId, _ in pairs(tShopList.tEquipment) do
    if WifEquipmentData.IsItemUnlocked(sId, sFaction) then
      nUnlocked = nUnlocked + 1
    end
  end
  Debug.Printf("Shop - Faction " .. sFaction .. " has " .. nUnlocked .. " items unlocked")
  return nUnlocked
end

function SaveSingleton()
  return _tGlobalShopList
end

function LoadSingleton(tData)
  _tGlobalShopList = tData
end
