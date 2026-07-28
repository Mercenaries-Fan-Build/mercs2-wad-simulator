inherit("MrxDaisyCutter")
import("MrxSupportDesignatorSmoke")
local sProjectileName = "MOAB Projectile"
local sExplosionName = "Explosion (MOAB)"
local sDeliveryVehicle = "Support Vehicle (C130)"

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetValidationFunction(_NoValidation)
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetModuleName("MrxMOAB")
  oNewSupport.sDeliveryVehicle = sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(sDeliveryVehicle)
  oNewSupport.sBomb = sProjectileName
  oNewSupport.uBomb = Pg.GetGuidByName(sProjectileName)
  return oNewSupport
end
