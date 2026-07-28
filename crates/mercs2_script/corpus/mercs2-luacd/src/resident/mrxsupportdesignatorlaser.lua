import("MrxSupport")
inherit("MrxSupportDesignator")
import("MrxSupportManager")
import("MrxPmc")

function Init()
  Pg.LoadAsset("global_weapon_laserrangefinder", "model")
end

function Deinit()
  Pg.UnloadAsset("global_weapon_laserrangefinder", "model")
end

function Create(self, oNewDesignator)
  oNewDesignator = oNewDesignator or {}
  oNewDesignator.uOwner = self.uOwner
  oNewDesignator.bDesignationComplete = self.bDesignationComplete
  oNewDesignator.sDesignationType = "Laser Designator"
  oNewDesignator.fValidationFunction = self.fValidationFunction
  oNewDesignator.tCallbackList = {}
  oNewDesignator.sAATestLevel = "medium"
  oNewDesignator.nX = self.nX
  oNewDesignator.nY = self.nY
  oNewDesignator.nZ = self.nZ
  oNewDesignator.uGuid = self.uGuid
  setmetatable(oNewDesignator, self)
  self.__index = self
  return oNewDesignator
end

function Commence(self, bFireImmediately)
  if "userdata" ~= type(self.uOwner) then
    return
  end
  return Airstrike.EquipDesignator(self.uOwner, self.sDesignationType, LaserFinished, {self}, false)
end

function LaserFinished(self, uGuid)
  if self.sAATestLevel and MrxSupport.TestAALevel(self.sAATestLevel) then
    MrxSupport.DenialMessage("aa")
    return
  end
  if self:GetParentSupport():GetFuelCost() > MrxPmc.GetFuelQty() then
    MrxSupport.DenialMessage("fuel")
    return
  end
  if not MrxSupportManager.IsRecruitAvailable(self:GetParentSupport():GetRecruit()) then
    return
  end
  local nX, nY, nZ = Object.GetPosition(uGuid)
  self:SetDesignationParameters(nX, nY, nZ, uGuid)
  self:CompleteDesignation()
  MrxSupportManager.StartRecruitCooldown(self:GetParentSupport():GetRecruit())
end

function ShouldSuppressIconAnimationOnDirectUse(self)
  return false
end

function GetType(self)
  return "laser"
end
