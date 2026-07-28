uOwner = nil
bDesignationComplete = false
sDesignationType = nil
sAATestLevel = nil
fValidationFunction = nil
tCallbackList = {}
tVOCues = {}
nX = nil
nY = nil
nZ = nil
uGuid = nil
oParentSupport = nil

function GetTarget(self)
  if not self.bDesignationComplete then
    return nil
  end
  return self.nX, self.nY, self.nZ, self.uGuid, self.uTarget
end

function RemoveBeacon(self)
  if self.sDesignationType and "Beacon Designator" == self.sDesignationType and self.uGuid then
    Object.Remove(self.uGuid)
  end
end

function Create(self, oNewDesignator)
  oNewDesignator = oNewDesignator or {}
  oNewDesignator.uOwner = self.uOwner
  oNewDesignator.bDesignationComplete = self.bDesignationComplete
  oNewDesignator.bDesignateOnDeath = true
  oNewDesignator.sDesignationType = self.sDesignationType
  oNewDesignator.sAATestLevel = self.sAATestLevel
  oNewDesignator.fValidationFunction = self.fValidationFunction
  oNewDesignator.tCallbackList = {}
  oNewDesignator.nX = self.nX
  oNewDesignator.nY = self.nY
  oNewDesignator.nZ = self.nZ
  oNewDesignator.uGuid = self.uGuid
  setmetatable(oNewDesignator, self)
  self.__index = self
  return oNewDesignator
end

function SetOwner(self, uPlayerGuid)
  if "table" ~= type(self) then
    return
  end
  if "userdata" ~= type(uPlayerGuid) then
    return
  end
  self.uOwner = uPlayerGuid
end

function AddCompleteCallback(self, fCallback, tCallbackData)
  self:SetCompleteCallback(fCallback, tCallbackData)
end

function RemoveCompleteCallback(self, fCallback)
  self.tCallbackList[fCallback] = nil
end

function SetCompleteCallback(self, fCallback, tCallbackData)
  if "function" == type(fCallback) then
    local tData = {}
    if "table" == type(tCallbackData) then
      tData = tCallbackData
    end
    self.tCallbackList[fCallback] = tData
  end
end

function SetDesignationType(self, sTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sTemplateName) and nil ~= sTemplateName then
    return
  end
  self.sDesignationType = sTemplateName
end

function SetValidationFunction(self, fFunction)
  if "table" ~= type(self) then
    return
  end
  if "function" ~= type(fFunction) and nil ~= fFunction then
    return
  end
  self.fValidationFunction = fFunction
end

function SetTargetValidationRequired(self, bRequireValidation)
  if "table" ~= type(self) then
    return
  end
  if "boolean" ~= type(bRequireValidation) then
    return
  end
  self.bValidateTarget = bRequireValidation
end

function SetAATestLevel(self, sAATestLevel)
  if "table" ~= type(self) then
    return
  end
  self.sAATestLevel = sAATestLevel
end

function SetTargetLocation(self, nX, nY, nZ)
  if "table" ~= type(self) then
    return
  end
  if "number" ~= type(nX) or "number" ~= type(nY) or "number" ~= type(nZ) then
    return
  end
  self.nX = nX
  self.nY = nY
  self.nZ = nZ
end

function OnDeny(self, uGuid)
end

function SetParentSupport(self, oSupport)
  self.oParentSupport = oSupport
end

function GetParentSupport(self, oSupport)
  return self.oParentSupport
end

function Configure(self, tOptions)
  for Index in pairs(tOptions) do
    self[Index] = tOptions[Index] or self[Index]
  end
end

function Commence(self, bFireImmediately)
  if "userdata" ~= type(self.uOwner) then
    return false
  end
  self.uWeaponGuid = Airstrike.EquipDesignator(self.uOwner, self.sDesignationType, nil, nil, bFireImmediately)
  if self.uWeaponGuid then
    Weapon.SetReserveAmmo(self.uWeaponGuid, 1)
  end
  return self.uWeaponGuid
end

function GetType(self)
  return "none"
end

local nWinchLength = 8
local _cargoTemplateData = {}
_cargoTemplateData.default = {
  nRadius = 3,
  nHeightTolerance = 2,
  bWater = false
}
_cargoTemplateData.Jetski = {
  nRadius = 3,
  nHeightTolerance = 2,
  bWater = true
}
_cargoTemplateData.box = {
  nRadius = 2,
  nHeightTolerance = 2,
  bWater = false
}
_cargoTemplateData["0x8000721F"] = _cargoTemplateData.box
_cargoTemplateData["0x80006252"] = _cargoTemplateData.box
_cargoTemplateData["0x8000721E"] = _cargoTemplateData.box
_cargoTemplateData["0x80007D92"] = _cargoTemplateData.box
_cargoTemplateData["0x80009AE8"] = _cargoTemplateData.box
_cargoTemplateData["0x80009ae9"] = _cargoTemplateData.box
_cargoTemplateData["0x8000721B"] = _cargoTemplateData.box
_cargoTemplateData["0x8000855D"] = _cargoTemplateData.box
_cargoTemplateData["0x80007221"] = _cargoTemplateData.box
_cargoTemplateData["0x8000A259"] = _cargoTemplateData.box
_cargoTemplateData["0x8000721C"] = _cargoTemplateData.box
_cargoTemplateData["0x80006251"] = _cargoTemplateData.box
_cargoTemplateData["0x8000721D"] = _cargoTemplateData.box
_cargoTemplateData["0x80007223"] = _cargoTemplateData.box
_cargoTemplateData["0x80008E5D"] = _cargoTemplateData.box
local _heliTemplateData = {}
_heliTemplateData.default = {
  nHeightMax = 16,
  nInnerRadius = 4,
  nOuterRadius = 13,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80008208"] = {
  nHeightMax = 16,
  nInnerRadius = 3,
  nOuterRadius = 13,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80009467"] = {
  nHeightMax = 20,
  nInnerRadius = 6,
  nOuterRadius = 19,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80009466"] = {
  nHeightMax = 20,
  nInnerRadius = 6,
  nOuterRadius = 19,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x800092C4"] = {
  nHeightMax = 16,
  nInnerRadius = 4,
  nOuterRadius = 11,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x800081FF"] = {
  nHeightMax = 16,
  nInnerRadius = 4,
  nOuterRadius = 11,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80008529"] = {
  nHeightMax = 16,
  nInnerRadius = 4,
  nOuterRadius = 13,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80008207"] = {
  nHeightMax = 16,
  nInnerRadius = 4,
  nOuterRadius = 13,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x800081FB"] = {
  nHeightMax = 14,
  nInnerRadius = 3,
  nOuterRadius = 8,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x800081FA"] = {
  nHeightMax = 14,
  nInnerRadius = 3,
  nOuterRadius = 8,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80008204"] = {
  nHeightMax = 14,
  nInnerRadius = 3,
  nOuterRadius = 8,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}
_heliTemplateData["0x80008686"] = _heliTemplateData["0x80008208"]
_heliTemplateData["0x80006F71"] = {
  nHeightMax = 29,
  nInnerRadius = 8,
  nOuterRadius = 24,
  nInnerHeightTolerance = 1,
  nOuterHeightTolerance = 2.5
}

function ValidateGroundDropZone(fCallback, nX, nY, nZ, oSupport, bWater)
  Debug.Printf("ValidateGroundDropZone")
  local oCargoData = _cargoTemplateData[tostring(oSupport.uCargoToDeliver)] or _cargoTemplateData.default
  local oHeliData = _heliTemplateData[tostring(oSupport.uDeliveryVehicle)] or _heliTemplateData.default
  local bDropZoneAdded = Ai.TestDropZone({
    Callback = fCallback,
    Location = {
      nX,
      nY,
      nZ
    },
    InnerRadius = oCargoData.nRadius,
    InnerHeightTolerance = oCargoData.nHeightTolerance,
    OuterRadius = oHeliData.nOuterRadius,
    OuterHeightTolerance = oHeliData.nOuterHeightTolerance + nWinchLength,
    HeightMax = oHeliData.nHeightMax + nWinchLength,
    SearchRadius = 6,
    Water = bWater
  })
  if not bDropZoneAdded then
    fCallback(false, "nodrop")
  end
end

function ValidateWaterDropZone(fCallback, nX, nY, nZ, oSupport)
  ValidateGroundDropZone(fCallback, nX, nY, nZ, oSupport, true)
end

function ValidateLandingZone(fCallback, nX, nY, nZ, oSupport)
  Debug.Printf("ValidateLandingZone")
  local oData = _heliTemplateData[tostring(oSupport.uDeliveryVehicle)] or _heliTemplateData.default
  local bDropZoneAdded = Ai.TestDropZone({
    Callback = fCallback,
    Location = {
      nX,
      nY,
      nZ
    },
    InnerRadius = 3,
    InnerHeightTolerance = 2,
    OuterRadius = oData.nOuterRadius,
    OuterHeightTolerance = oData.nOuterHeightTolerance + 8,
    HeightMax = oData.nHeightMax,
    SearchRadius = 20,
    Water = false
  })
  if not bDropZoneAdded then
    fCallback(false, "noland")
  end
end

function CompleteDesignation(self)
  self.bDesignationComplete = true
  for fFunction, tData in pairs(self.tCallbackList) do
    fFunction(unpack(tData))
  end
  Event.Post("Airstrike", {
    sStage = "DesignationComplete",
    sType = "None"
  })
end

function SetDesignationParameters(self, nNewX, nNewY, nNewZ, uGuid, uTarget)
  if "number" ~= type(nNewX) then
    nNewX = nil
  end
  if "number" ~= type(nNewY) then
    nNewY = nil
  end
  if "number" ~= type(nNewZ) then
    nNewZ = nil
  end
  if "userdata" ~= type(uGuid) then
    uGuid = nil
  end
  if "userdata" ~= type(uTarget) then
    uTarget = nil
  end
  self.nX = nNewX or self.nX
  self.nY = nNewY or self.nY
  self.nZ = nNewZ or self.nZ
  self.uGuid = uGuid or self.uGuid
  self.uTarget = uTarget or self.uTarget
end
