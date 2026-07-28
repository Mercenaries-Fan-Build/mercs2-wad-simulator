inherit("MrxSupportDelivery")
import("MrxSupportDesignator")

function Create(oSelf, uOwnerGuid)
  local oNewSupport = MrxSupportDelivery:Create(uOwnerGuid)
  oNewSupport.Create = Create
  oNewSupport:SetCargo(oSelf.sCargoToDeliver)
  oNewSupport:SetDeliveryVehicle(oSelf.sDeliveryVehicle)
  oNewSupport:SetCareless(oSelf.bCareless)
  local oDesignator = oNewSupport:GetDesignator()
  oDesignator:SetValidationFunction(MrxSupportDesignator.ValidateGroundDropZone)
  oDesignator:SetSmokeColor("blue")
  oDesignator:SetAATestLevel("none")
  oNewSupport:SetModuleName("MrxCrateDelivery")
  return oNewSupport
end
