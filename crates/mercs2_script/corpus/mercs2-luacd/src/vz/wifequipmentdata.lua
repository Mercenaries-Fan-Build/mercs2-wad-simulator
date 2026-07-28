knTypeFuelTank = 1
knTypeCostume = 2
knTypeGrapplingHook = 3
_knUnlockStatusNew = 1
_knUnlockStatusViewed = 2
_tEquipment = {
  FuelTank1 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 1
  },
  FuelTank2 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 2
  },
  FuelTank3 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 3
  },
  FuelTank4 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 4
  },
  FuelTank5 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 5
  },
  FuelTank6 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 6
  },
  FuelTank7 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 7
  },
  FuelTank8 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 100000,
    nFuelCapacity = 200,
    nFuelTankId = 8
  },
  FuelTank9 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 250000,
    nFuelCapacity = 700,
    nFuelTankId = 9
  },
  FuelTank10 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 250000,
    nFuelCapacity = 700,
    nFuelTankId = 10
  },
  FuelTank11 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 250000,
    nFuelCapacity = 700,
    nFuelTankId = 11
  },
  FuelTank12 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 250000,
    nFuelCapacity = 700,
    nFuelTankId = 12
  },
  FuelTank13 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 250000,
    nFuelCapacity = 700,
    nFuelTankId = 13
  },
  FuelTank14 = {
    sName = "[Generic.FuelSilo]",
    sDescription = "[Generic.FuelSiloDescription]",
    sTexture = "support_bombing_run",
    nType = knTypeFuelTank,
    nCost = 250000,
    nFuelCapacity = 700,
    nFuelTankId = 14
  },
  GrapplingHook = {
    sName = "[weapon.grapple]",
    sDescription = "[Fiona.Grapple01]",
    sTexture = "weapons_grappling",
    nType = knTypeGrapplingHook,
    nCost = 100000
  }
}

function GetEquipmentData(sId)
  return _tEquipment[sId]
end

function UnlockItem(vId, sFactionId)
  if type(vId) == "table" then
    for _, sId in ipairs(vId) do
      _UnlockItem(sId, sFactionId)
    end
  else
    _UnlockItem(vId, sFactionId)
  end
end

function _UnlockItem(sId, sFactionId)
  local tEquipmentData = GetEquipmentData(sId)
  if tEquipmentData then
    tEquipmentData.tUnlockStatus = tEquipmentData.tUnlockStatus or {}
    if not tEquipmentData.tUnlockStatus[sFactionId] then
      tEquipmentData.tUnlockStatus[sFactionId] = _knUnlockStatusNew
    end
  end
end

function IsItemUnlocked(sId, sFaction)
  local tEquipmentData = GetEquipmentData(sId)
  if tEquipmentData.tUnlockStatus then
    return tEquipmentData.tUnlockStatus[sFaction] ~= nil
  end
  return false
end

function IsItemNew(sId, sFaction)
  local tEquipmentData = GetEquipmentData(sId)
  if tEquipmentData.tUnlockStatus then
    return tEquipmentData.tUnlockStatus[sFaction] == _knUnlockStatusNew
  end
  return false
end

function SetItemViewed(sId, sFaction)
  local tEquipmentData = GetEquipmentData(sId)
  if tEquipmentData.tUnlockStatus and tEquipmentData.tUnlockStatus[sFaction] == _knUnlockStatusNew then
    tEquipmentData.tUnlockStatus[sFaction] = _knUnlockStatusViewed
  end
end

function GetPlayerVisibleName(sEquipmentId)
  local tEquipmentData = GetEquipmentData(sEquipmentId)
  if tEquipmentData then
    return tEquipmentData.sName
  end
end

function SaveSingleton()
  local tSaveData = {}
  for sId, tEquipmentData in pairs(_tEquipment) do
    tSaveData[sId] = tEquipmentData.tUnlockStatus
  end
  return tSaveData
end

function LoadSingleton(tData)
  for sId, tUnlockStatus in pairs(tData) do
    _tEquipment[sId].tUnlockStatus = tUnlockStatus
  end
end
